// Deny usage of print and eprint as it won't have same result
// in WASI as if doing in standard program, you must really know
// what you are doing to disable that lint (and you don't know)
#![deny(clippy::print_stdout)]
#![deny(clippy::print_stderr)]

use anyhow::{Result, anyhow};
use lapce_plugin::{
    psp_types::{
        lsp_types::{request::Initialize, DocumentFilter, DocumentSelector, InitializeParams, Url, MessageType},
        Request,
    },
    register_plugin, LapcePlugin, VoltEnvironment, PLUGIN_RPC,
};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Default)]
struct State {}

register_plugin!(State);

fn initialize(params: InitializeParams) -> Result<()> {
    let document_selector: DocumentSelector = vec![
        DocumentFilter {
            language: None, // This alone doesn't work, should be with pattern
            pattern: Some(String::from("**/*.thrift")),
            scheme: None,
        },
        DocumentFilter {
            // lsp language id
            language: Some(String::from("thrift")),
            // glob pattern
            pattern: None,
            // like file:
            scheme: None,
        }
    ];
    let mut server_args = vec![];

    // Check for user specified LSP server path
    // ```
    // [lapce-plugin-name.lsp]
    // serverPath = "[path or filename]"
    // serverArgs = ["--arg1", "--arg2"]
    // ```
    if let Some(options) = params.initialization_options.as_ref() {
        if let Some(lsp) = options.get("lsp") {
            if let Some(args) = lsp.get("serverArgs") {
                if let Some(args) = args.as_array() {
                    if !args.is_empty() {
                        server_args = vec![];
                    }
                    for arg in args {
                        if let Some(arg) = arg.as_str() {
                            server_args.push(arg.to_string());
                        }
                    }
                }
            }

            if let Some(server_path) = lsp.get("serverPath") {
                if let Some(server_path) = server_path.as_str() {
                    if !server_path.is_empty() {
                        let server_uri = Url::parse(&format!("urn:{}", server_path))?;
                        let _ = PLUGIN_RPC.start_lsp(
                            server_uri,
                            server_args,
                            document_selector,
                            params.initialization_options,
                        );
                        return Ok(());
                    }
                }
            }
        }
    }

    // Architecture check
    let arch = match VoltEnvironment::architecture().as_deref() {
        Ok("x86_64") => "amd64",
        Ok("aarch64") => "arm64",
        _ => return Ok(()),
    };

    // OS check
    let os = match VoltEnvironment::operating_system().as_deref() {
        Ok("macos") => "darwin",
        Ok("linux") => "linux",
        Ok("windows") => "windows",
        _ => return Ok(()),
    };

    let filename = format!("thriftls-{}-{}", os, arch);

    let filename = match VoltEnvironment::operating_system().as_deref() {
        Ok("windows") => {
            format!("{}.exe", filename)
        }
        _ => filename.to_string(),
    };

    // Download URL
    let download_uri = format!("https://github.com/joyme123/thrift-ls/releases/download/v0.1.0/{filename}");

    // Plugin working directory
    let volt_uri = VoltEnvironment::uri()?;
    let server_uri = Url::parse(&volt_uri)?.join(filename.as_str())?;

    let file_path = PathBuf::from(server_uri.path());
    if !file_path.exists() {
        // see lapce_plugin::Http for available API to download files
        let mut download_res = lapce_plugin::Http::get(download_uri.as_str())?;

        if download_res.status_code.as_u16() / 100 != 2 {
            let err_msg = download_res.body_read_all()?;
            let err_msg = String::from_utf8(err_msg)?;
            // download error
            return Err(anyhow!(format!("download error: {}", err_msg)));
        } 

        let body = download_res.body_read_all()?;
        std::fs::write(file_path, body)?;
    }

    // if you want to use server from PATH
    // let server_uri = Url::parse(&format!("urn:{filename}"))?;

    // Available language IDs
    // https://github.com/lapce/lapce/blob/HEAD/lapce-proxy/src/buffer.rs#L173
    let _ = PLUGIN_RPC.start_lsp(
        server_uri,
        server_args,
        document_selector,
        params.initialization_options,
    );

    Ok(())
}

impl LapcePlugin for State {
    fn handle_request(&mut self, _id: u64, method: String, params: Value) {
        #[allow(clippy::single_match)]
        match method.as_str() {
            Initialize::METHOD => {
                let params: InitializeParams = serde_json::from_value(params).unwrap();
                if let Err(e) = initialize(params) {
                    let _ = PLUGIN_RPC.window_show_message(MessageType::ERROR, format!("plugin returned with error: {e}"));
                }
            }
            _ => {}
        }
    }
}
