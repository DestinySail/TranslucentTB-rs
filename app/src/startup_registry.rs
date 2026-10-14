use windows::{core::{w, BSTR, Error, PCWSTR, PWSTR, Result, VARIANT}, Win32::{
    Foundation::*, Security::{*, Authorization::ConvertSidToStringSidW},
    System::{Com::*, Ole::*, Threading::*, Wmi::*},
}};

struct Apartment;
impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize(); } } }

// Use Windows' registry provider for startup approval. On this machine a direct
// RegGetValue/RegDeleteValue round trip disagreed with Task Manager and StdRegProv.
// Target the process user's SID explicitly; never use the provider service's HKCU.
pub struct Registry {
    service: IWbemServices,
    class: IWbemClassObject,
    sid: String,
    _apartment: Apartment,
}

fn required<T>(value: Option<T>) -> Result<T> {
    value.ok_or_else(|| Error::new(E_FAIL, "Missing registry provider response"))
}

fn user_sid() -> Result<String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
        let token = clearbar_common::Handle(token);
        let mut size = 0;
        let _ = GetTokenInformation(token.0, TokenUser, None, 0, &mut size);
        if size == 0 { return Err(Error::from_win32()); }
        let mut data = vec![0usize; (size as usize + size_of::<usize>() - 1) / size_of::<usize>()];
        GetTokenInformation(token.0, TokenUser, Some(data.as_mut_ptr().cast()), size, &mut size)?;
        let user = &*data.as_ptr().cast::<TOKEN_USER>();
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text)?;
        let sid = text.to_string();
        let _ = LocalFree(HLOCAL(text.0.cast()));
        Ok(sid?)
    }
}

impl Registry {
    pub fn open() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let apartment = Apartment;
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
            let service = locator.ConnectServer(&BSTR::from("ROOT\\DEFAULT"), &BSTR::new(),
                &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None)?;
            // Default authentication service/authorization, with the current user's token.
            CoSetProxyBlanket(&service, u32::MAX, u32::MAX, None,
                RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE)?;
            let mut class = None;
            service.GetObject(&BSTR::from("StdRegProv"), WBEM_FLAG_RETURN_WBEM_COMPLETE,
                None, Some(&mut class), None)?;
            Ok(Self { service, class: required(class)?, sid: user_sid()?, _apartment: apartment })
        }
    }

    fn call(&self, method: PCWSTR, path: &str, value: &str) -> Result<(u32, IWbemClassObject)> {
        unsafe {
            let mut signature = None;
            self.class.GetMethod(method, 0, &mut signature, std::ptr::null_mut())?;
            let input = required(signature)?.SpawnInstance(0)?;
            // HKU + exact current-user SID, restricted to Clearbar's own values by callers.
            // WMI represents CIM_UINT32 method parameters as VT_I4 (not VT_UI4).
            input.Put(w!("hDefKey"), 0, &VARIANT::from(0x80000003u32 as i32), 0)?;
            input.Put(w!("sSubKeyName"), 0, &VARIANT::from(format!("{}\\{path}", self.sid).as_str()), 0)?;
            input.Put(w!("sValueName"), 0, &VARIANT::from(value), 0)?;
            let mut output = None;
            let name = BSTR::from_wide(method.as_wide())?;
            self.service.ExecMethod(&BSTR::from("StdRegProv"), &name, WBEM_FLAG_RETURN_WBEM_COMPLETE,
                None, &input, Some(&mut output), None)?;
            let output = required(output)?;
            let mut code = VARIANT::new();
            output.Get(w!("ReturnValue"), 0, &mut code, None, None)?;
            Ok((u32::try_from(&code)?, output))
        }
    }

    pub fn first_byte(&self, path: &str, value: &str) -> Result<Option<u8>> {
        let (code, output) = self.call(w!("GetBinaryValue"), path, value)?;
        // StdRegProv reports a missing value as 1 or 2 depending on the provider.
        if matches!(code, 1 | 2 | 3) { return Ok(None); }
        WIN32_ERROR(code).ok()?;
        unsafe {
            let mut data = VARIANT::new();
            output.Get(w!("uValue"), 0, &mut data, None, None)?;
            let raw = &data.as_raw().Anonymous.Anonymous;
            if raw.vt != 0x2011 { return Err(Error::new(E_FAIL, "Invalid startup approval array")); }
            let array = raw.Anonymous.parray.cast::<SAFEARRAY>();
            let index = SafeArrayGetLBound(array, 1)?;
            if SafeArrayGetUBound(array, 1)? < index { return Ok(Some(255)); }
            let mut byte = 0u8;
            SafeArrayGetElement(array, &index, (&mut byte as *mut u8).cast())?;
            Ok(Some(byte))
        }
    }

    pub fn delete(&self, path: &str, value: &str) -> Result<()> {
        let (code, _) = self.call(w!("DeleteValue"), path, value)?;
        if matches!(code, 2 | 3) { Ok(()) } else { WIN32_ERROR(code).ok() }
    }
}
