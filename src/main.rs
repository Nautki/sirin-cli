use std::process::{self, exit};
use std::thread::sleep;
use std::time::Duration;
use std::io::Read;

use clap::{Parser, Subcommand};

use derive_more::{Display, From};
use rusb::{Device, DeviceHandle, GlobalContext};
use sirin_shared::packet::{ByteArrayStrError, CallsignBuf, InPacket, NicknameBuf, OutPacket, MAX_OUT_PACKET_SIZE};
use sirin_shared::song::{FromSong, FromSongError, SongSize, ToSong, ToSongError};
use sirin_shared::usb::{USB_EP_IN_ADDR, USB_EP_OUT_ADDR, USB_PID, USB_VID};
use sirin_shared::packet::ByteArrayStr;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Subcommands
}

#[derive(Subcommand)]
enum Subcommands {
    #[command(about = "View and set configuration")]
    Config {
        #[arg(long)]
        nickname: Option<String>,

        #[arg(long)]
        callsign: Option<String>,
    },
    #[command(about = "See the outgoing Sirin packets")]
    Tail,
    #[command(about = "List connected Sirins")]
    Ls
}

fn main() {
    let args = Args::parse();

    match run(args) {
        Ok(_) => {},
        Err(err) => {
            println!("{}", err);
            exit(1)
        }
    }
}

fn run(args: Args) -> Result<(), Error> {
    match args.command {
        Subcommands::Config { nickname, callsign } => {
            let is_changing_config = nickname.is_some() || callsign.is_some();

            let sirin = SirinDev::connect()?.open()?;
            sirin.write_packet(&InPacket::QueryConfig)?;
            
            let mut config = loop {
                let packet = sirin.read_packet()?;
                let OutPacket::Config(config) = packet else {
                    continue;
                };

                break config;
            };

            if is_changing_config {
                println!("Old config:");
            }
            println!("{}", config);

            if is_changing_config {
                if let Some(nickname) = nickname {
                    config.nickname = NicknameBuf::from_str(&nickname)?;
                }

                if let Some(callsign) = callsign {
                    config.callsign = CallsignBuf::from_str(&callsign)?;
                }

                println!("New config:");
                println!("{}", config);

                sirin.write_packet(&InPacket::SetConfig(config))?;


            }
        }
        Subcommands::Ls => {
            SirinDev::list();
        },
        Subcommands::Tail => {
            let sirin = SirinDev::connect()?.open()?;
            
            loop {
                match sirin.read_packet() {
                    Ok(packet) => println!("{:?}", packet),
                    Err(Error::NoSirin) => return Err(Error::NoSirin),
                    Err(err) => println!("Error: {}", err)
                }
            }
        }
    }

    Ok(())
}

pub struct SirinDev {
    device: Device<GlobalContext>
}

#[derive(From, Display)]
pub enum Error {
    #[display("No Sirin found.")]
    NoSirin,
    #[display("There was an error with the USB: {_0}")]
    UsbError(rusb::Error),
    #[from]
    #[display("There was an error decoding the packet: {_0:?}")]
    FromSongError(FromSongError),
    #[from]
    #[display("There was an error encoding the packet: {_0:?}")]
    ToSongError(ToSongError),
    #[from]
    #[display("There was an error with a byte array string: {_0:?}")]
    ByteArrayStr(ByteArrayStrError)
}

impl From<rusb::Error> for Error {
    fn from(value: rusb::Error) -> Self {
        match value {
            rusb::Error::NoDevice => Self::NoSirin,
            err => Self::UsbError(err)
        }
    }
}

impl SirinDev {
    // TODO: make DRY with `connect()`
    fn list() {
        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();

            if device_desc.vendor_id() == USB_VID && device_desc.product_id() == USB_PID {
                let Ok(handler) = device.open() else {
                    continue;
                };

                println!("Bus {:03} Device {:03} ID {:04x}:{:04x} {} {} {}",
                    device.bus_number(),
                    device.address(),
                    device_desc.vendor_id(),
                    device_desc.product_id(),
                    handler.read_manufacturer_string_ascii(&device_desc).unwrap_or_default(),
                    handler.read_product_string_ascii(&device_desc).unwrap_or_default(),
                    handler.read_serial_number_string_ascii(&device_desc).unwrap_or_default()
                );
            }
        }
    }

    fn connect() -> Result<Self, Error> {
        for device in rusb::devices().unwrap().iter() {
            let device_desc = device.device_descriptor().unwrap();

            if device_desc.vendor_id() == USB_VID && device_desc.product_id() == USB_PID {
                println!("Connected to Sirin:");
                let handler = device.open()?;

                println!("Bus {:03} Device {:03} ID {:04x}:{:04x} {} {} {}",
                    device.bus_number(),
                    device.address(),
                    device_desc.vendor_id(),
                    device_desc.product_id(),
                    handler.read_manufacturer_string_ascii(&device_desc).unwrap_or_default(),
                    handler.read_product_string_ascii(&device_desc).unwrap_or_default(),
                    handler.read_serial_number_string_ascii(&device_desc).unwrap_or_default()
                );
                
                return Ok(Self {
                    device
                })
            }
        }

        return Err(Error::NoSirin)
    }

    pub fn open(&self) -> Result<SirinHandle, Error> {
        let dev = self.device.open()?;
        dev.claim_interface(0)?;
        Ok(SirinHandle { dev })
    }
}

pub struct SirinHandle {
    dev: DeviceHandle<GlobalContext>
}

impl SirinHandle {
    pub fn read_packet(&self) -> Result<OutPacket, Error> {
        let mut buf = [0u8; MAX_OUT_PACKET_SIZE * 128 * 100];
        let len = self.dev.read_bulk(USB_EP_IN_ADDR, &mut buf, Duration::from_secs(0))?;
        Ok(OutPacket::from_song(&buf[0..len])?)
    }

    pub fn write_packet(&self, packet: &InPacket) -> Result<(), Error> {
        let mut buf = [0u8; MAX_OUT_PACKET_SIZE];
        packet.to_song(&mut buf)?;
        self.dev.write_bulk(USB_EP_OUT_ADDR, &mut buf[0..packet.song_size()], Duration::from_secs(0))?;
        Ok(())
    }
}