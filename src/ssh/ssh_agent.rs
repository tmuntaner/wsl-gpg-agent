use crate::ssh::file_mapping::FileMapping;
use crate::ssh::pageant_window::PageantWindow;
use anyhow::Result;
use std::{io, process};

pub struct SshAgent {}

impl SshAgent {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run(
        &self,
        pageant_window_name: &str,
        pageant_class_name: &str,
        stdout: &mut dyn io::Write,
        stdin: &mut dyn io::BufRead,
    ) -> Result<()> {
        // build shared memory map
        let map_name = format!("WSLPageantRequest{}", process::id());
        let file_mapping = FileMapping::new(&map_name)?;
        let mut map_view_of_file = file_mapping.shared_memory()?;
        let shared_memory_slice = map_view_of_file.shared_memory();

        // write request to shared memory
        let request = self.read_request(stdin)?;
        for (i, value) in request.iter().enumerate() {
            shared_memory_slice[i] = *value;
        }

        // send message to pageant saying we've written to our shared memory
        let pageant_window = PageantWindow::new(pageant_window_name, pageant_class_name)?;
        pageant_window.send_message(&map_name)?;

        // send the result to stdout
        self.send_result(stdout, shared_memory_slice)?;

        Ok(())
    }

    fn read_request(&self, stdin: &mut dyn io::BufRead) -> Result<Vec<u8>> {
        let mut length_buffer = [0u8; 4];
        stdin.read_exact(&mut length_buffer)?;
        let length = u32::from_be_bytes(length_buffer);

        let mut buffer = vec![0u8; length as usize];
        stdin.read_exact(&mut buffer)?;

        let mut request = vec![0u8; (length + 4) as usize];
        for (i, value) in length_buffer.iter().enumerate() {
            request[i] = *value;
        }
        for (i, value) in buffer.iter().enumerate() {
            request[i + 4] = *value;
        }

        Ok(request)
    }

    fn send_result(
        &self,
        stdout: &mut dyn io::Write,
        shared_memory_slice: &mut [u8],
    ) -> Result<()> {
        let length_buffer: [u8; 4] = [
            shared_memory_slice[0],
            shared_memory_slice[1],
            shared_memory_slice[2],
            shared_memory_slice[3],
        ];
        let length = u32::from_be_bytes(length_buffer);

        let mut result = vec![0u8; (length + 4) as usize];
        for i in 0..(length + 4) {
            result[i as usize] = shared_memory_slice[i as usize];
        }

        stdout.write_all(result.as_slice())?;
        stdout.flush()?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_send_result() {
        let ssh = SshAgent::new();
        let mut stdout = Vec::new();
        let length = 6_u32.to_be_bytes();
        let mut data = [
            length[0], length[1], length[2], length[3], 0u8, 1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8,
        ];

        ssh.send_result(&mut stdout, &mut data).unwrap();
        assert_eq!(10, stdout.len()); // length of 6 + 4 for u32.to_be_bytes
        for n in 0..10 {
            assert_eq!(&data[n], stdout.get(n).unwrap());
        }
    }
}
