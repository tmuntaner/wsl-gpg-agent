use crate::ssh::AGENT_MAX_LENGTH;
use anyhow::{bail, Result};
use std::ffi::c_void;
use std::slice;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Memory::{MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS};

pub struct SharedMemory<'a> {
    pointer: *mut c_void,
    shared_memory: &'a mut [u8],
}

impl SharedMemory<'_> {
    pub fn new(file_mapping: HANDLE) -> Result<Self> {
        let shared_memory;
        unsafe {
            shared_memory = MapViewOfFile(file_mapping, FILE_MAP_ALL_ACCESS, 0u32, 0u32, 0_usize);
        }

        if shared_memory.is_null() {
            bail!("failed MapViewOfFile");
        }

        let shared_memory_slice;
        unsafe {
            shared_memory_slice =
                slice::from_raw_parts_mut(shared_memory as *mut _, AGENT_MAX_LENGTH as usize);
        }

        Ok(Self {
            pointer: shared_memory,
            shared_memory: shared_memory_slice,
        })
    }

    pub fn shared_memory(&mut self) -> &mut [u8] {
        self.shared_memory
    }
}

impl Drop for SharedMemory<'_> {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.pointer);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ssh::file_mapping::FileMapping;
    use std::process;

    #[test]
    fn test_new() {
        let map_name = format!("WSLPageantRequest-test-map-file-of-view{}", process::id());
        let file_mapping = FileMapping::new(map_name.as_str()).unwrap();
        let mut shared_memory = file_mapping.shared_memory().unwrap();

        let shared_slice = shared_memory.shared_memory();
        assert_eq!(AGENT_MAX_LENGTH as usize, shared_slice.len());

        let result = SharedMemory::new(HANDLE::default());
        assert!(result.is_err());
    }
}
