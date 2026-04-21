use interprocess::local_socket::tokio::Stream;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::ListenerOptions;

fn build() {
    let _ = ListenerOptions::new().name(r"\\.\pipe\test".into()).create_tokio();
}
