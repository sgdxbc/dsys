use std::env::args;

pub mod client;
pub mod replica;

fn main() {
    match args().nth(1).as_deref() {
        Some("replica") => replica::main(),
        Some("client") => client::main(
            args()
                .nth(2)
                .as_deref()
                .unwrap_or("127.0.0.1")
                .parse()
                .unwrap(),
        ),
        _ => panic!(),
    }
}
