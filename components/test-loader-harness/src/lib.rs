#![no_main]

use crate::{
    componentized::component::{path_loader, types::Error},
    exports::wasi::cli::run::Guest,
    wasi::cli::{environment, stderr, stdout},
};

/// Writes `$bytes` (a `Vec<u8>`) to a wasi:cli output stream (`stdout` or `stderr`).
///
/// Must be used within an async context. Evaluates to `Result<(), ()>`, which is `Err`
/// if the host reports an error or stops reading before all bytes are written.
macro_rules! write_to {
    ($stream:ident, $bytes:expr) => {
        async {
            let (mut tx, rx) = wit_stream::new();
            // hand the reader to the host before writing, otherwise the write never completes
            let result = $stream::write_via_stream(rx);
            let remaining = tx.write_all($bytes).await;
            // close the stream so the host sees EOF and resolves `result`
            drop(tx);
            match result.await {
                Ok(()) if remaining.is_empty() => Ok(()),
                _ => Err(()),
            }
        }
        .await
    };
}

struct LoaderHarness;

impl Guest for LoaderHarness {
    #[doc = " Run the program."]
    #[allow(async_fn_in_trait)]
    async fn run() -> Result<(), ()> {
        let args = environment::get_arguments();
        let path = args.get(1).expect("path argument required");

        match path_loader::load(path.to_string()).await {
            Ok(component) => {
                write_to!(stdout, component)
            }
            Err(e) => {
                let message = match e {
                    Error::Other(Some(message)) => message,
                    Error::Other(None) => "--unknown error--".to_string(),
                };
                let _ = write_to!(stderr, format!("{}\n", message).into_bytes());
                Err(())
            }
        }
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "test-loader-harness",
    merge_structurally_equal_types: true,
    generate_all
});

export!(LoaderHarness);
