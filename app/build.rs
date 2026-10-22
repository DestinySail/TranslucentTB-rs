use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const APP_ICON_ID: u16 = 1;
const RESOURCE_LANGUAGE: u16 = 0x0409;
const RESOURCE_MEMORY_FLAGS: u16 = 0x1030;
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;

fn main() {
    println!("cargo:rerun-if-env-changed=CLEARBAR_TAP_DLL");
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let dll = std::env::var_os("CLEARBAR_TAP_DLL").map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../target/release/clearbar_tap.dll"));
    let dll = dll.canonicalize().expect("Build the embedded TAP first: cargo build --release -p clearbar-tap --locked");
    println!("cargo:rerun-if-changed={}", dll.display());
    println!("cargo:rustc-env=CLEARBAR_EMBEDDED_TAP={}", dll.display());

    embed_icon(&manifest);
}

struct IconImage<'a> {
    width: u8,
    height: u8,
    color_count: u8,
    reserved: u8,
    planes: u16,
    bit_count: u16,
    data: &'a [u8],
}

fn embed_icon(manifest: &Path) {
    let icon = manifest.join("assets/clearbar.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let bytes = fs::read(&icon).unwrap_or_else(|error| panic!("Read {}: {error}", icon.display()));
    let resources = build_resources(&bytes).unwrap_or_else(|error| panic!("Build icon resources: {error}"));
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("clearbar-icon.res");
    fs::write(&output, resources).unwrap_or_else(|error| panic!("Write {}: {error}", output.display()));
    // LLD accepts RC's .res format directly, so the build needs no external resource compiler.
    println!("cargo:rustc-link-arg-bins={}", output.display());
}

fn build_resources(ico: &[u8]) -> Result<Vec<u8>, String> {
    if ico.len() < 6 || read_u16(ico, 0)? != 0 || read_u16(ico, 2)? != 1 {
        return Err("clearbar.ico is not a valid ICO file".into());
    }
    let count = read_u16(ico, 4)? as usize;
    if count == 0 || count > u16::MAX as usize {
        return Err("clearbar.ico contains no supported images".into());
    }

    let mut images = Vec::with_capacity(count);
    for index in 0..count {
        let entry = 6 + index * 16;
        let size = read_u32(ico, entry + 8)? as usize;
        let offset = read_u32(ico, entry + 12)? as usize;
        let end = offset.checked_add(size).ok_or("Icon image offset overflow")?;
        if offset < 6 + count * 16 || end > ico.len() {
            return Err(format!("Icon image {index} is outside clearbar.ico"));
        }
        images.push(IconImage {
            width: ico[entry],
            height: ico[entry + 1],
            color_count: ico[entry + 2],
            reserved: ico[entry + 3],
            planes: read_u16(ico, entry + 4)?,
            bit_count: read_u16(ico, entry + 6)?,
            data: &ico[offset..end],
        });
    }

    let mut group = Vec::with_capacity(6 + images.len() * 14);
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (index, image) in images.iter().enumerate() {
        group.push(image.width);
        group.push(image.height);
        group.push(image.color_count);
        group.push(image.reserved);
        group.extend_from_slice(&image.planes.max(1).to_le_bytes());
        group.extend_from_slice(&image.bit_count.to_le_bytes());
        group.extend_from_slice(&(image.data.len() as u32).to_le_bytes());
        group.extend_from_slice(&((index + 1) as u16).to_le_bytes());
    }

    // .res starts with a null record; this is the same layout emitted by rc.exe.
    let mut resources = resource_record(0, 0, &[]);
    for (index, image) in images.iter().enumerate() {
        resources.extend(resource_record(RT_ICON, (index + 1) as u16, image.data));
    }
    resources.extend(resource_record(RT_GROUP_ICON, APP_ICON_ID, &group));
    Ok(resources)
}

fn resource_record(resource_type: u16, name: u16, data: &[u8]) -> Vec<u8> {
    let mut record = Vec::with_capacity(32 + data.len() + 3);
    record.extend_from_slice(&(data.len() as u32).to_le_bytes());
    record.extend_from_slice(&32u32.to_le_bytes());
    record.extend_from_slice(&resource_id(resource_type).to_le_bytes());
    record.extend_from_slice(&resource_id(name).to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(&RESOURCE_MEMORY_FLAGS.to_le_bytes());
    record.extend_from_slice(&RESOURCE_LANGUAGE.to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(data);
    while record.len() % 4 != 0 { record.push(0); }
    record
}

fn resource_id(value: u16) -> u32 {
    ((value as u32) << 16) | 0xffff
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes.get(offset..offset + 2).ok_or("Truncated ICO header")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes.get(offset..offset + 4).ok_or("Truncated ICO header")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}
