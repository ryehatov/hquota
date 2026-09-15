use hquota::{
    cli::{self, Command},
    protocol,
};
use std::{io::Write, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let result = (|| {
        let command = cli::parse(std::env::args_os().skip(1))?;
        if let Command::Serve { config } = command {
            hquota::broker::serve(&config)?;
            return Ok(true);
        }
        let (output, success) = hquota::client::run(command, Path::new(protocol::SOCKET))?;
        std::io::stdout()
            .lock()
            .write_all(output.as_bytes())
            .map_err(|_| "output_failed")?;
        Ok::<_, &'static str>(success)
    })();
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(code) => {
            eprintln!("hquota: {code}");
            ExitCode::FAILURE
        }
    }
}
