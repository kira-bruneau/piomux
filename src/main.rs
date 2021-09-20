mod ring_buffer;
mod server;

use crate::server::Server;

use std::{env, io};

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let mut args = env::args_os();
    args.next();

    match args.next() {
        Some(subcommand) => match subcommand.to_string_lossy().as_ref() {
            "listen" => {
                Server::listen(
                    args.next().expect("Missing socket path"),
                    args.next().expect("Missing program"),
                    args,
                )?
                .await
            }
            "connect" => todo!(),
            subcommand => panic!("Invalid subcommand: {}", subcommand),
        },
        None => panic!("Missing subcommand"),
    }
}
