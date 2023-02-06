use anyhow::{anyhow, bail, Result};
use clap::Parser;
use futures::{future, SinkExt, StreamExt};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;
use std::{fs, str};
use tokio::io::{self, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::select;
use tokio_util::codec::{BytesCodec, FramedRead, FramedWrite};

#[derive(Parser)]
pub struct Gpg {}

impl Gpg {
    pub fn run(&self) -> Result<()> {
        let gpg_agent_path = dirs::cache_dir()
            .ok_or_else(|| anyhow!("could not determine cache directory"))?
            .join("gnupg")
            .join("S.gpg-agent");
        let (port, nonce) = get_gpg_port(gpg_agent_path)?;

        let runtime = tokio::runtime::Runtime::new()?;

        let _ = runtime.block_on(async {
            let addr = format!("localhost:{port}");
            let socket = TcpStream::connect(addr).await?;
            let (rd, mut wr) = io::split(socket);

            let writer = tokio::spawn(async move {
                wr.write_all(nonce.as_slice()).await?;

                let stdin = FramedRead::new(tokio::io::stdin(), BytesCodec::new());
                let mut sink = FramedWrite::new(wr, BytesCodec::new());
                let mut stdin = stdin.map(|i| i.map(|bytes| bytes.freeze()));

                sink.send_all(&mut stdin).await?;

                Ok::<_, io::Error>(())
            });

            let reader = tokio::spawn(async move {
                let mut stdout = FramedWrite::new(io::stdout(), BytesCodec::new());
                // filter map Result<BytesMut, Error> stream into just a Bytes stream to match stdout Sink
                // on the event of an Error, log the error and end the stream
                let mut stream = FramedRead::new(rd, BytesCodec::new())
                    .filter_map(|i| match i {
                        //BytesMut into Bytes
                        Ok(i) => future::ready(Some(i.freeze())),
                        Err(e) => {
                            println!("failed to read from socket; error={e}");
                            future::ready(None)
                        }
                    })
                    .map(Ok);

                stdout.send_all(&mut stream).await?;

                Ok::<_, io::Error>(())
            });

            select! {
                _ = reader => {},
                _ = writer => {}
            };

            Ok::<_, io::Error>(())
        });

        // stdin is blocking, so we need to force a shutdown
        // https://github.com/tokio-rs/tokio/issues/2466
        runtime.shutdown_timeout(Duration::from_secs(0));

        Ok(())
    }
}

fn get_gpg_port(path: PathBuf) -> Result<(String, Vec<u8>)> {
    let mut f = File::open(&path)?;
    let metadata = fs::metadata(path)?;
    let mut buffer = vec![0u8; metadata.len() as usize];
    f.read_exact(&mut buffer)?;

    let line_size = buffer
        .iter()
        .take_while(|c| **c != b'\n' && **c != b'\r')
        .count();
    let port = str::from_utf8(&buffer[..line_size])?.to_string();
    let nonce = buffer[(line_size + 1)..].to_vec();
    if nonce.len() != 16 {
        bail!("nonce should be 16 bytes");
    }

    Ok((port, nonce))
}
