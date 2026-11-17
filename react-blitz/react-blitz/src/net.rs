//! A build's files, read straight from disk: its stylesheets, fonts and images arrive in the same
//! turn they are asked for, so a page is whole once it has been resolved once.

use std::sync::Arc;

use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};

/// Answers `file:` and `data:` addresses; anything else fails, since a build is all on disk.
#[derive(Default)]
pub struct FileNetProvider;

impl FileNetProvider {
    pub fn shared() -> Arc<dyn NetProvider> {
        Arc::new(Self)
    }
}

impl NetProvider for FileNetProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url.to_string();
        match read(&request) {
            Ok(bytes) => handler.bytes(url, bytes),
            Err(error) => handler.error(url, error),
        }
    }
}

fn read(request: &Request) -> Result<Bytes, String> {
    match request.url.scheme() {
        "file" => {
            let path = request
                .url
                .to_file_path()
                .map_err(|()| format!("{} names no file", request.url))?;
            std::fs::read(&path)
                .map(Bytes::from)
                .map_err(|e| format!("{}: {e}", path.display()))
        }
        "data" => {
            let data = data_url::DataUrl::process(request.url.as_str()).map_err(|e| format!("{e:?}"))?;
            let (body, _) = data.decode_to_vec().map_err(|e| format!("{e:?}"))?;
            Ok(Bytes::from(body))
        }
        scheme => Err(format!("{scheme}: is not read by a built page; only file: and data:")),
    }
}
