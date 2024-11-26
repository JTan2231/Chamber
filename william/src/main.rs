use chamber_common::{error, Logger};

fn main() {
    let server = std::net::TcpListener::bind("127.0.0.1:9001").unwrap();
    println!("WebSocket server listening on ws://127.0.0.1:9001");

    for stream in server.incoming() {
        std::thread::spawn(move || {
            let stream = stream.unwrap();
            let mut websocket = tungstenite::accept(stream).unwrap();

            loop {
                let msg = match websocket.read() {
                    Ok(m) => m,
                    Err(e) => {
                        error!("error reading from websocket: {}", e);
                        continue;
                    }
                };

                if msg.is_close() {
                    break;
                }

                println!("{}", msg);

                websocket.send(msg).unwrap();
            }
        });
    }
}
