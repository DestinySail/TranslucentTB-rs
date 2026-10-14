//! Classic IObjectWithSite: keep XAML Diagnostics' original COM apartment semantics.
use std::{cell::RefCell, ffi::c_void, sync::atomic::{AtomicU32, Ordering}};
use windows::{core::{GUID, HRESULT, Interface, IUnknown, IUnknown_Vtbl}, Win32::{Foundation::*, System::Ole::IObjectWithSite}};
use crate::{boundary, subscribe};
#[repr(C)]
struct Vtable {
    base: IUnknown_Vtbl,
    set: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    get: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
}
#[repr(C)]
struct Site { vtable: *const Vtable, refs: AtomicU32, site: RefCell<Option<IUnknown>> }
static VTABLE: Vtable = Vtable {
    base: IUnknown_Vtbl { QueryInterface: query, AddRef: add_ref, Release: release }, set, get,
};
unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if iid.is_null() || out.is_null() { return E_POINTER; }
    *out = std::ptr::null_mut();
    if *iid == IUnknown::IID || *iid == IObjectWithSite::IID {
        *out = this; add_ref(this); S_OK
    } else { E_NOINTERFACE }
}
unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    (*(this as *mut Site)).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    let remaining = (*(this as *mut Site)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if remaining == 0 { drop(Box::from_raw(this as *mut Site)); }
    remaining
}
unsafe extern "system" fn set(this: *mut c_void, object: *mut c_void) -> HRESULT {
    boundary(|| {
        let site = &*(this as *mut Site);
        let value = IUnknown::from_raw_borrowed(&object).cloned();
        *site.site.borrow_mut() = value.clone();
        if let Some(object) = value { subscribe(&object)?; }
        Ok(())
    })
}
unsafe extern "system" fn get(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    boundary(|| {
        if iid.is_null() || out.is_null() { return Err(E_POINTER.into()); }
        *out = std::ptr::null_mut();
        let site = (*(this as *mut Site)).site.borrow();
        site.as_ref().ok_or_else(|| windows::core::Error::from(E_FAIL))?.query(iid, out).ok()
    })
}
pub fn create() -> IUnknown {
    let raw = Box::into_raw(Box::new(Site { vtable: &VTABLE, refs: AtomicU32::new(1), site: RefCell::new(None) }));
    unsafe { IUnknown::from_raw(raw.cast()) }
}
