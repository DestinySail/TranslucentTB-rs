//! Narrow ABI prefixes from Windows SDK xamlom.h / Windows.UI.Xaml metadata.
#![allow(non_snake_case, non_camel_case_types)]
use std::ffi::c_void;
use windows_core::{interface, BSTR, HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, GUID};
#[repr(C)]
pub struct SourceInfo {
    pub FileName: BSTR, pub LineNumber: u32, pub ColumnNumber: u32,
    pub CharPosition: u32, pub Hash: BSTR,
}
#[repr(C)]
pub struct ParentChildRelation { pub Parent: u64, pub Child: u64, pub ChildIndex: u32 }
#[repr(C)]
pub struct VisualElement {
    pub Handle: u64, pub SrcInfo: SourceInfo,
    pub Type: BSTR, pub Name: BSTR, pub NumChildren: u32,
}
#[interface("aa7a8931-80e4-4fec-8f3b-553f87b4966e")]
pub unsafe trait IVisualTreeServiceCallback: IUnknown {
    pub fn OnVisualTreeChange(&self, relation: ParentChildRelation, element: VisualElement, mutation: i32) -> HRESULT;
}
// Flatten the inherited ABI prefix to avoid the macro's custom inheritance issue.
// QueryInterface exposes both IIDs with their SDK-compatible vtable layouts.
#[interface("bad9eb88-ae77-4397-b948-5fa2db0a19ea")]
pub unsafe trait IVisualTreeServiceCallback2: IUnknown {
    pub fn OnVisualTreeChange(&self, relation: ParentChildRelation, element: VisualElement, mutation: i32) -> HRESULT;
    pub fn OnElementStateChanged(&self, element: u64, state: i32, context: PCWSTR) -> HRESULT;
}
#[interface("0e79c6e0-85a0-4be8-b41a-655cf1fd19bd")]
pub unsafe trait IVisualTreeService3: IUnknown {
    pub fn AdviseVisualTreeChange(&self, callback: *mut c_void) -> HRESULT;
    pub fn UnadviseVisualTreeChange(&self, callback: *mut c_void) -> HRESULT;
}
#[interface("18c9e2b6-3f43-4116-9f2b-ff935d7770d2")]
pub unsafe trait IXamlDiagnostics: IUnknown {
    pub fn GetDispatcher(&self, object: *mut *mut c_void) -> HRESULT;
    pub fn GetUiLayer(&self, object: *mut *mut c_void) -> HRESULT;
    pub fn GetApplication(&self, object: *mut *mut c_void) -> HRESULT;
    pub fn GetIInspectableFromHandle(&self, handle: u64, object: *mut *mut c_void) -> HRESULT;
}
#[interface("676d0be9-b65c-41c6-ba40-58cf87f201c1")]
pub unsafe trait IUIElement: IUnknown {
    pub fn GetIids(&self, count: *mut u32, iids: *mut *mut GUID) -> HRESULT;
    pub fn GetRuntimeClassName(&self, name: *mut *mut u16) -> HRESULT;
    pub fn GetTrustLevel(&self, trust: *mut i32) -> HRESULT;
    pub fn GetDesiredSize(&self, size: *mut c_void) -> HRESULT;
    pub fn GetAllowDrop(&self, value: *mut bool) -> HRESULT;
    pub fn SetAllowDrop(&self, value: bool) -> HRESULT;
    pub fn GetOpacity(&self, value: *mut f64) -> HRESULT;
    pub fn SetOpacity(&self, value: f64) -> HRESULT;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn x64_sdk_structure_layout() {
        assert_eq!(std::mem::size_of::<SourceInfo>(), 32);
        assert_eq!(std::mem::size_of::<ParentChildRelation>(), 24);
        assert_eq!(std::mem::size_of::<VisualElement>(), 64);
        assert_eq!(std::mem::offset_of!(VisualElement, Type), 40);
        assert_eq!(std::mem::offset_of!(IUIElement_Vtbl, GetOpacity), 9 * 8);
    }
}
