use std::time::Duration;
use std::io::Read;

fn main() {
    let port_name = "/dev/cu.usbmodem21301"; 
    let baud_rate = 115200; 

    let mut port = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_millis(1000))
        .open()
        .expect("Failed to open serial port");
    let mut buffer = [0u8; 64];

    loop {
        match port.read(&mut buffer) {
            Ok(n) => {
                println!("{:?}", String::from_utf8_lossy(&buffer[..n]));
            }
            Err(e) => {
                eprintln!("{}", e);
            }
        }
    }
}