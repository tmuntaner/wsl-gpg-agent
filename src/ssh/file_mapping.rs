use crate::ssh::shared_memory::SharedMemory;
use crate::ssh::AGENT_MAX_LENGTH;
use anyhow::Result;
use widestring::U16CString;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::System::Memory::{CreateFileMappingW, PAGE_EXECUTE_READWRITE};

pub struct FileMapping {
    handle: HANDLE,
}

impl FileMapping {
    pub fn new(map_name: &str) -> Result<Self> {
        let map_name_u16 = U16CString::from_str(map_name)?;
        let map_name_u16 = PCWSTR(map_name_u16.as_ptr() as *mut u16);

        let file_mapping;
        unsafe {
            file_mapping = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null::<SECURITY_ATTRIBUTES>(),
                PAGE_EXECUTE_READWRITE,
                0,
                AGENT_MAX_LENGTH,
                map_name_u16,
            )?;
        }

        Ok(Self {
            handle: file_mapping,
        })
    }

    pub fn shared_memory(&self) -> Result<SharedMemory> {
        SharedMemory::new(self.handle)
    }
}

impl Drop for FileMapping {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::process;
    use windows::Win32::System::Threading::CreateMutexW;

    #[test]
    fn test_new() {
        let map_name = format!("WSLPageantRequest-create-file-mapping-1{}", process::id());
        let file_mapping = FileMapping::new(map_name.as_str()).unwrap();
        assert_ne!(0, file_mapping.handle.0);

        // you cannot create a file mapping if its name is already taken by a mutex
        // let's create one
        let map_name = format!("WSLPageantRequest-create-file-mapping-2{}", process::id());
        let map_name_c_string = U16CString::from_str(map_name.clone()).unwrap();
        let map_name_c_string = PCWSTR(map_name_c_string.as_ptr() as *mut u16);
        let mutex: HANDLE;
        unsafe {
            mutex = CreateMutexW(
                std::ptr::null::<SECURITY_ATTRIBUTES>(),
                true,
                map_name_c_string,
            )
            .unwrap();
        }
        assert_ne!(0, mutex.0);

        // now the create file mapping should fail
        let file_mapping = FileMapping::new(map_name.as_str());
        assert!(file_mapping.is_err());

        unsafe {
            CloseHandle(mutex);
        }
    }
}
