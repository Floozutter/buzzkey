use midir::{MidiInput, MidiInputPort};
use wmidi::MidiMessage;
use buttplug::{
    client::{
        ButtplugClient, ButtplugClientEvent, ButtplugClientDeviceMessageType, 
        VibrateCommand,
    },
    server::ButtplugServerOptions,
};
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::runtime::Handle;
use futures::{StreamExt, Stream};
use std::{error::Error, convert::TryFrom, collections::HashMap, io::Write};

fn prompt_midi(
    client_name: &str
) -> Result<(MidiInput, MidiInputPort), Box<dyn Error>> {
    let imidi = MidiInput::new(client_name)?;
    let mut iports = imidi.ports();
    match iports.len() {
        0 => Err("no available MIDI input port found".into()),
        1 => {
            println!(
                "selecting only available MIDI input port: {}",
                imidi.port_name(&iports[0])?
            );
            Ok((imidi, iports.pop().unwrap()))
        },
        _ => {
            println!("available input ports:");
            for (i, p) in iports.iter().enumerate() {
                println!("{}: {}", i, imidi.port_name(p)?);
            }
            print!("select input port: ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            Ok((
                imidi,
                iports.into_iter().nth(
                    input.trim().parse::<usize>()?
                ).ok_or("invalid input port selected")?
            ))
        },
    }
}

async fn handle_scanning(mut event_stream: impl Stream<Item = ButtplugClientEvent> + Unpin) {
    loop {
        match event_stream.next().await.unwrap() {
            ButtplugClientEvent::DeviceAdded(dev) => {
                tokio::spawn(async move {
                    println!("device added: {}", dev.name);
                });
            },
            ButtplugClientEvent::ScanningFinished => {
                println!("scanning finished signaled!");
                return;
            },
            ButtplugClientEvent::ServerDisconnect => {
                println!("server disconnected!");
            },
            _ => {
                println!("something happened!");
            },
        }
    };
}

async fn run() -> Result<(), Box<dyn Error>> {
    // select MIDI input port
    let (imidi, iport) = prompt_midi("buzzkey midir input")?;
    // connect Buttplug devices
    let client = ButtplugClient::new("buzzkey buttplug client");
    let event_stream = client.event_stream();
    client.connect_in_process(&ButtplugServerOptions::default()).await?;
    client.start_scanning().await?;
    let scan_handler = tokio::spawn(handle_scanning(event_stream));
    println!("\nscanning for devices! press enter at any point to stop scanning and connect MIDI.");
    BufReader::new(io::stdin()).lines().next_line().await?;
    client.stop_scanning().await?;
    scan_handler.await?;
    // connect to MIDI input port
    let handle = Handle::current();
    let mut notes = HashMap::new();
    let _iport_connection = imidi.connect(&iport, "buzzkey_iport", move |_, bytes, _| {
        if let Some((c, n, p)) = match MidiMessage::try_from(bytes) {
            Ok(MidiMessage::NoteOn(c, n, v)) => Some((c, n, u8::from(v))),
            Ok(MidiMessage::NoteOff(c, n, _)) => Some((c, n, 0)),
            _ => None,
        } {
            notes.insert((c, n), p);
            let sum: u32 = notes.values().map(|&p| p as u32).sum();
            let speed = (sum as f64 / 254.0).max(0.0).min(1.0);
            println!("{} {}", sum, speed);
            for dev in client.devices() {
                handle.spawn(async move {
                    if dev.allowed_messages.contains_key(&ButtplugClientDeviceMessageType::VibrateCmd) {
                        dev.vibrate(VibrateCommand::Speed(speed)).await.unwrap();
                    }
                });
            }
        }
    }, ())?;
    println!("\nconnected MIDI input to device output! press enter at any point to quit.");
    BufReader::new(io::stdin()).lines().next_line().await?;
    println!("bye-bye! >:3c");
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {}!", err);
    }
}
