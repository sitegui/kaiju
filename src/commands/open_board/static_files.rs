use crate::commands::open_board::ApiError;
use anyhow::Result;
use axum::http::header;
use axum::response::IntoResponse;
use std::fs;

#[derive(Debug, Clone, Copy)]
pub enum StaticSource {
    CompileTime,
    RunTime,
}

#[derive(Debug, Clone, Copy)]
pub enum StaticFile {
    IndexHtml,
    IndexJs,
    IndexCss,
    Favicon,
}

impl StaticFile {
    pub(super) fn serve(self, source: StaticSource) -> Result<impl IntoResponse, ApiError> {
        let content = self.content(source)?;

        let header = [(header::CONTENT_TYPE, self.content_type())];
        Ok((header, content))
    }

    fn content_type(self) -> &'static str {
        match self {
            StaticFile::IndexHtml => "text/html",
            StaticFile::IndexJs => "text/javascript",
            StaticFile::IndexCss => "text/css",
            StaticFile::Favicon => "image/png",
        }
    }

    fn content(self, source: StaticSource) -> Result<Vec<u8>> {
        match source {
            StaticSource::CompileTime => {
                let bytes = match self {
                    StaticFile::IndexHtml => {
                        include_bytes!("../../../resources/web/index.html").as_slice()
                    }
                    StaticFile::IndexJs => {
                        include_bytes!("../../../resources/web/index.js").as_slice()
                    }
                    StaticFile::IndexCss => {
                        include_bytes!("../../../resources/web/index.css").as_slice()
                    }
                    StaticFile::Favicon => {
                        include_bytes!("../../../resources/web/favicon.png").as_slice()
                    }
                };

                Ok(bytes.to_owned())
            }
            StaticSource::RunTime => {
                let path = match self {
                    StaticFile::IndexHtml => "resources/web/index.html",
                    StaticFile::IndexJs => "resources/web/index.js",
                    StaticFile::IndexCss => "resources/web/index.css",
                    StaticFile::Favicon => "resources/web/favicon.png",
                };

                let bytes = fs::read(path)?;
                Ok(bytes)
            }
        }
    }
}
