use std::time::Duration;
use std::io::Read;

use serialport::available_ports;
use sirin_shared::song::{FromSong, OutPacket, MAX_OUT_PACKET_SIZE};

fn main() {
    //let available_ports = available_ports().unwrap();
    //for available_port in available_ports {
    //    print!("{}", available_port.port_name);
    //s}
    let port_name = "/dev/cu.usbmodem21301"; 
    let baud_rate = 115200; 

    let mut port = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_millis(2000))
        .open()
        .expect("Failed to open serial port");
    let mut buffer = [0u8; MAX_OUT_PACKET_SIZE * 2];

    loop {
        let mut i = 0;

        loop {
            match port.read(&mut buffer[i..]) {
                Ok(n) => {
                    i += n;
    
                    if n == 32 {
                        continue;
                    }
    
                    let res = OutPacket::from_song(&buffer[0..i]);
                    println!("{:?}", &buffer[0..i]);
                    println!("{:?}", res);
                    break;
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                }
            }
        }
    }
} 