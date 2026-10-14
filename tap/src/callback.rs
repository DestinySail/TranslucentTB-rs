//! A classic, non-agile COM callback matching the original project's semantics.
use std::{ffi::c_void, sync::atomic::{AtomicU32, Ordering}};
use windows::Win32::Foundation::{E_NOINTERFACE, E_POINTER, S_OK};
use windows_core::{GUID, HRESULT, Interface, IUnknown, IUnknown_Vtbl, PCWSTR};
use crate::{abi::*, boundary, state};

#[repr(C)]
struct Vtable {
    base: IUnknown_Vtbl,
    change: unsafe extern "system" fn(*mut c_void, ParentChildRelation, VisualElement, i32) -> HRESULT,
    state: unsafe extern "system" fn(*mut c_void, u64, i32, PCWSTR) -> HRESULT,
}
#[repr(C)]
struct Callback { vtable: *const Vtable, refs: AtomicU32, diagnostics: IXamlDiagnostics }
static VTABLE: Vtable = Vtable {
    base: IUnknown_Vtbl { QueryInterface: query, AddRef: add_ref, Release: release },
    change, state: element_state,
};

unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if iid.is_null() || out.is_null() { return E_POINTER; }
    *out = std::ptr::null_mut();
    if *iid == IUnknown::IID || *iid == IVisualTreeServiceCallback::IID || *iid == IVisualTreeServiceCallback2::IID {
        *out = this; add_ref(this); S_OK
    } else { E_NOINTERFACE }
}
unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    (*(this as *mut Callback)).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    let remaining = (*(this as *mut Callback)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if remaining == 0 { drop(Box::from_raw(this as *mut Callback)); }
    remaining
}
unsafe extern "system" fn change(this: *mut c_void, relation: ParentChildRelation, element: VisualElement, mutation: i32) -> HRESULT {
    boundary(|| { state::change(&(*(this as *mut Callback)).diagnostics, relation, &element, mutation); Ok(()) })
}
unsafe extern "system" fn element_state(_: *mut c_void, _: u64, _: i32, _: PCWSTR) -> HRESULT { S_OK }

pub fn create(diagnostics: IXamlDiagnostics) -> IUnknown {
    let raw = Box::into_raw(Box::new(Callback { vtable: &VTABLE, refs: AtomicU32::new(1), diagnostics }));
    unsafe { IUnknown::from_raw(raw.cast()) }
}
