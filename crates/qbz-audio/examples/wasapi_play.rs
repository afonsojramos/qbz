//! Throwaway proof for the WASAPI exclusive path (phase B, task 2 step 8).
//!
//! Opens a render endpoint in EXCLUSIVE mode and plays a 1 kHz sine, then
//! reports what the stream negotiated and how many underruns it took.
//!
//! Deliberately quiet: -20 dBFS. A stability proof does not need full scale,
//! and this runs on somebody's actual monitors.
//!
//!   cargo run -p qbz-audio --example wasapi_play                 list endpoints
//!   cargo run -p qbz-audio --example wasapi_play <id> [rate] [s] play
//!
//! What it proves that a unit test cannot: the DAC's own display shows the
//! rate we asked for. If the display says 48 kHz while this says 96, the
//! stream is being resampled somewhere and it is NOT bit-perfect.

fn main() {
    #[cfg(not(windows))]
    {
        eprintln!("wasapi_play is Windows-only");
    }

    #[cfg(windows)]
    {
        use qbz_audio::wasapi_direct::{WasapiDirectStream, WasapiTiming};

        let args: Vec<String> = std::env::args().skip(1).collect();

        if args.is_empty() {
            list_endpoints();
            eprintln!();
            eprintln!("pass an endpoint id to play, e.g.");
            eprintln!("  cargo run -p qbz-audio --example wasapi_play \"{{0.0.0...}}\" 96000 10");
            return;
        }

        let endpoint = args[0].clone();
        let rate: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(96_000);
        let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);

        let stream = match WasapiDirectStream::new(&endpoint, rate, 2, WasapiTiming::Events) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("open failed: {e}");
                return;
            }
        };
        let info = stream.open_info();
        println!("endpoint : {}", info.endpoint_name);
        println!("rate     : {} Hz", info.rate);
        println!("format   : {:?} ({}/{} bits)", info.rung, info.rung.valid_bits(), info.rung.container_bits());
        println!("period   : {} hns   buffer: {} frames", info.period_hns, info.buffer_frames);
        println!("realigned: {}   timing: {:?}", info.realigned, info.timing);
        println!("bit-perfect mode: {:?}", stream.bit_perfect_mode());
        println!();
        println!("playing {secs}s of 1 kHz at -20 dBFS - CHECK THE DAC DISPLAY, it must read {rate}");

        // Feed in period-sized chunks so the queue's back-pressure paces us
        // instead of a sleep. 1 kHz divides every rate in the ladder evenly
        // enough that the phase never accumulates audible drift over 10 s.
        let frames_per_chunk = info.buffer_frames.max(1) as usize;
        let total_frames = rate as u64 * secs;
        let amp = 0.1f32; // -20 dBFS
        let step = 2.0 * std::f32::consts::PI * 1000.0 / rate as f32;

        let mut phase = 0.0f32;
        let mut written: u64 = 0;
        while written < total_frames {
            let n = frames_per_chunk.min((total_frames - written) as usize);
            let mut buf = Vec::with_capacity(n * 2);
            for _ in 0..n {
                let v = phase.sin() * amp;
                phase += step;
                if phase > std::f32::consts::TAU {
                    phase -= std::f32::consts::TAU;
                }
                buf.push(v); // L
                buf.push(v); // R
            }
            if let Err(e) = stream.write_f32(&buf) {
                eprintln!("write failed after {written} frames: {e}");
                break;
            }
            written += n as u64;
        }

        let _ = stream.drain();
        std::thread::sleep(std::time::Duration::from_millis(500));
        println!();
        println!("frames written : {written}");
        println!("underruns      : {}", stream.underruns());
        println!("event timeouts : {}", stream.event_timeouts());
        println!(
            "VERDICT        : {}",
            if stream.underruns() == 0 && stream.event_timeouts() == 0 {
                "PASS - no underruns, no timeouts"
            } else {
                "underruns occurred; try WasapiTiming::Polling"
            }
        );
    }
}

#[cfg(windows)]
fn list_endpoints() {
    use wasapi::{initialize_mta, Direction, DeviceEnumerator};
    let _ = initialize_mta().ok();
    let Ok(en) = DeviceEnumerator::new() else {
        eprintln!("DeviceEnumerator failed");
        return;
    };
    let Ok(coll) = en.get_device_collection(&Direction::Render) else {
        eprintln!("device collection failed");
        return;
    };
    let n = coll.get_nbr_devices().unwrap_or(0);
    println!("render endpoints:");
    for i in 0..n {
        if let Ok(d) = coll.get_device_at_index(i) {
            println!(
                "  [{i}] {}",
                d.get_friendlyname().unwrap_or_else(|_| "<unnamed>".into())
            );
            println!("       {}", d.get_id().unwrap_or_else(|_| "<no id>".into()));
        }
    }
}
