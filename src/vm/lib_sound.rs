//! `use vyrn.sound`: sound effects and music.
//!
//! ```vyrn
//! use vyrn.sound
//!
//! main:
//!     let shot = sound.load("shot.wav")
//!     let music = sound.load("theme.ogg")
//!     let song = music.play(volume = 0.4, loop = true)
//!     shot.play()
//!     # ... later
//!     song.stop()
//! end
//! ```
//!
//! Files are decoded once when loaded (WAV, Ogg Vorbis, MP3), so playing a
//! sound many times a second costs nothing. Sounds play at the same time,
//! mixed together, without waiting. `sound.tone` makes beeps and retro
//! effects without any file. On a computer with no sound device, playing
//! is silent rather than an error, so games still run.

use super::ops::{err, type_error, Ctx, VmResult};
use super::value::Value;

#[cfg(feature = "sound")]
mod imp {
    use super::*;
    use crate::vm::foreign::Foreign;
    use rodio::buffer::SamplesBuffer;
    use rodio::Source;
    use std::cell::RefCell;
    use std::num::NonZero;
    use std::rc::Rc;

    thread_local! {
        /// The sound device, opened the first time a sound plays; `None`
        /// inside when there is none.
        static DEVICE: RefCell<Option<Option<rodio::MixerDeviceSink>>> = const { RefCell::new(None) };
        /// Sounds still playing, kept so they play to the end even when
        /// nobody holds their handle.
        static PLAYING: RefCell<Vec<Rc<rodio::Player>>> = const { RefCell::new(Vec::new()) };
    }

    /// ALSA prints its own complaints (such as "cannot find card '0'" on a
    /// computer without sound) unless given a handler that stays quiet.
    #[cfg(target_os = "linux")]
    fn quiet_alsa() {
        use std::os::raw::{c_char, c_int};
        type Handler =
            unsafe extern "C" fn(*const c_char, c_int, *const c_char, c_int, *const c_char);
        unsafe extern "C" fn quiet(
            _: *const c_char,
            _: c_int,
            _: *const c_char,
            _: c_int,
            _: *const c_char,
        ) {
        }
        #[link(name = "asound")]
        unsafe extern "C" {
            fn snd_lib_error_set_handler(handler: Option<Handler>) -> c_int;
        }
        // SAFETY: the handler ignores its arguments, so the variadic tail
        // ALSA passes is never read.
        unsafe {
            snd_lib_error_set_handler(Some(quiet));
        }
    }

    fn with_device<T>(f: impl FnOnce(Option<&rodio::MixerDeviceSink>) -> T) -> T {
        DEVICE.with(|d| {
            let mut d = d.borrow_mut();
            if d.is_none() {
                #[cfg(target_os = "linux")]
                quiet_alsa();
                let mut sink = rodio::DeviceSinkBuilder::open_default_sink().ok();
                if let Some(s) = &mut sink {
                    s.log_on_drop(false);
                }
                *d = Some(sink);
            }
            f(d.as_ref().and_then(|s| s.as_ref()))
        })
    }

    /// A decoded sound: samples, interleaved by channel.
    pub struct Sound {
        pub channels: u16,
        pub rate: u32,
        pub samples: std::sync::Arc<[f32]>,
    }

    impl Sound {
        fn seconds(&self) -> f64 {
            self.samples.len() as f64 / f64::from(self.channels) / f64::from(self.rate)
        }

        fn buffer(&self) -> SamplesBuffer {
            SamplesBuffer::new(
                NonZero::new(self.channels).unwrap_or(NonZero::<u16>::MIN),
                NonZero::new(self.rate).unwrap_or(NonZero::<u32>::MIN),
                self.samples.to_vec(),
            )
        }

        /// Writes a 16-bit WAV file.
        fn save_wav(&self, path: &str) -> std::io::Result<()> {
            let data_len = (self.samples.len() * 2) as u32;
            let mut out = Vec::with_capacity(44 + data_len as usize);
            out.extend_from_slice(b"RIFF");
            out.extend_from_slice(&(36 + data_len).to_le_bytes());
            out.extend_from_slice(b"WAVEfmt ");
            out.extend_from_slice(&16u32.to_le_bytes());
            out.extend_from_slice(&1u16.to_le_bytes());
            out.extend_from_slice(&self.channels.to_le_bytes());
            out.extend_from_slice(&self.rate.to_le_bytes());
            let block = self.channels * 2;
            out.extend_from_slice(&(self.rate * u32::from(block)).to_le_bytes());
            out.extend_from_slice(&block.to_le_bytes());
            out.extend_from_slice(&16u16.to_le_bytes());
            out.extend_from_slice(b"data");
            out.extend_from_slice(&data_len.to_le_bytes());
            for s in self.samples.iter() {
                let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                out.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(path, out)
        }
    }

    fn kw<'a>(kwargs: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
        kwargs.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    fn num(v: &Value, what: &str) -> VmResult<f64> {
        match v {
            Value::Int(i) => Ok(*i as f64),
            Value::Float(f) => Ok(*f),
            other => Err(type_error(format!(
                "{what} must be a number, not {}",
                other.type_name()
            ))),
        }
    }

    impl Foreign for Sound {
        fn type_name(&self) -> &'static str {
            "Sound"
        }

        fn display(&self) -> String {
            format!("Sound({:.2} seconds)", self.seconds())
        }

        fn get_attr(&self, name: &str) -> VmResult<Value> {
            match name {
                "duration" => Ok(Value::Float(self.seconds())),
                "channels" => Ok(Value::Int(i64::from(self.channels))),
                "sample_rate" => Ok(Value::Int(i64::from(self.rate))),
                _ => Err(err("FieldError", format!("a Sound has no field `{name}`"))),
            }
        }

        fn call_method(
            &self,
            _: &mut Ctx,
            name: &str,
            args: &[Value],
            kwargs: &[(String, Value)],
        ) -> VmResult<Value> {
            match name {
                // `snd.play(volume = 1.0, loop = false, speed = 1.0)`
                "play" => {
                    let volume = match kw(kwargs, "volume").or(args.first()) {
                        Some(v) => num(v, "the volume")?,
                        None => 1.0,
                    };
                    let looping = kw(kwargs, "loop")
                        .or(args.get(1))
                        .is_some_and(Value::truthy);
                    let speed = match kw(kwargs, "speed").or(args.get(2)) {
                        Some(v) => num(v, "the speed")?,
                        None => 1.0,
                    };
                    let player = with_device(|device| {
                        let device = device?;
                        let player = rodio::Player::connect_new(device.mixer());
                        if looping {
                            player.append(self.buffer().repeat_infinite());
                        } else {
                            player.append(self.buffer());
                        }
                        player.set_volume(volume as f32);
                        player.set_speed(speed as f32);
                        Some(Rc::new(player))
                    });
                    if let Some(p) = &player {
                        PLAYING.with(|list| {
                            let mut list = list.borrow_mut();
                            list.retain(|p| !p.empty());
                            list.push(p.clone());
                        });
                    }
                    Ok(Value::Foreign(Rc::new(Box::new(Playing { player }))))
                }
                // `snd.save("beep.wav")`
                "save" => {
                    let path = match args.first() {
                        Some(Value::Str(s)) => s.to_string(),
                        _ => return Err(type_error("`save` takes a path ending in .wav")),
                    };
                    self.save_wav(&path)
                        .map_err(|e| err("IoError", format!("cannot write {path}: {e}")))?;
                    Ok(Value::None)
                }
                _ => Err(err("FieldError", format!("a Sound has no method `{name}`"))),
            }
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// A sound that is playing (or, without a sound device, silently
    /// pretending to).
    pub struct Playing {
        player: Option<Rc<rodio::Player>>,
    }

    impl Foreign for Playing {
        fn type_name(&self) -> &'static str {
            "Playing"
        }

        fn display(&self) -> String {
            format!(
                "Playing({})",
                if self.is_playing() { "on" } else { "stopped" }
            )
        }

        fn get_attr(&self, name: &str) -> VmResult<Value> {
            match name {
                "playing" => Ok(Value::Bool(self.is_playing())),
                "volume" => Ok(Value::Float(
                    self.player.as_ref().map_or(0.0, |p| f64::from(p.volume())),
                )),
                _ => Err(err(
                    "FieldError",
                    format!("a Playing sound has no field `{name}`"),
                )),
            }
        }

        fn call_method(
            &self,
            _: &mut Ctx,
            name: &str,
            args: &[Value],
            _: &[(String, Value)],
        ) -> VmResult<Value> {
            let Some(p) = &self.player else {
                return Ok(Value::None);
            };
            match name {
                "stop" => p.stop(),
                "pause" => p.pause(),
                "resume" => p.play(),
                "set_volume" => {
                    p.set_volume(num(args.first().unwrap_or(&Value::Int(1)), "the volume")? as f32)
                }
                "set_speed" => {
                    p.set_speed(num(args.first().unwrap_or(&Value::Int(1)), "the speed")? as f32)
                }
                _ => {
                    return Err(err(
                        "FieldError",
                        format!("a Playing sound has no method `{name}`"),
                    ))
                }
            }
            Ok(Value::None)
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    impl Playing {
        fn is_playing(&self) -> bool {
            self.player
                .as_ref()
                .is_some_and(|p| !p.empty() && !p.is_paused())
        }
    }

    pub fn load(path: &str) -> VmResult<Value> {
        let file = std::fs::File::open(path)
            .map_err(|e| err("IoError", format!("cannot read {path}: {e}")))?;
        let decoder = rodio::Decoder::try_from(file).map_err(|e| {
            err(
                "ValueError",
                format!("{path} is not a sound this can play (WAV, Ogg Vorbis or MP3): {e}"),
            )
        })?;
        let channels = decoder.channels().get();
        let rate = decoder.sample_rate().get();
        let samples: Vec<f32> = decoder.collect();
        Ok(wrap(Sound {
            channels,
            rate,
            samples: samples.into(),
        }))
    }

    pub fn wrap(s: Sound) -> Value {
        Value::Foreign(Rc::new(Box::new(s)))
    }

    pub fn available() -> bool {
        with_device(|d| d.is_some())
    }

    pub fn stop_all() {
        PLAYING.with(|list| {
            for p in list.borrow().iter() {
                p.stop();
            }
            list.borrow_mut().clear();
        });
    }
}

/// `sound.load(path)`: a sound file, decoded and ready to play.
pub fn load(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => return Err(type_error("`sound.load` takes the file's path")),
    };
    #[cfg(feature = "sound")]
    {
        imp::load(&path)
    }
    #[cfg(not(feature = "sound"))]
    {
        let _ = path;
        Err(unavailable())
    }
}

#[cfg(not(feature = "sound"))]
fn unavailable() -> Value {
    err(
        "Unavailable",
        "`vyrn.sound` needs a build with sound (the `sound` feature, on by default)",
    )
}

/// The samples of a tone: `wave` is "sine", "square", "triangle", "saw"
/// or "noise"; a short fade at both ends avoids clicks.
pub fn tone_samples(
    frequency: f64,
    seconds: f64,
    wave: &str,
    volume: f64,
) -> Result<Vec<f32>, String> {
    const RATE: f64 = 44100.0;
    let n = (seconds.max(0.0) * RATE) as usize;
    let fade = (RATE * 0.005) as usize;
    let mut noise: u32 = 0x1234_5678;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / RATE;
        let phase = (t * frequency).fract();
        let v = match wave {
            "sine" => (phase * std::f64::consts::TAU).sin(),
            "square" => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            "triangle" => 1.0 - 4.0 * (phase - 0.5).abs(),
            "saw" => 2.0 * phase - 1.0,
            "noise" => {
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                f64::from(noise) / f64::from(u32::MAX) * 2.0 - 1.0
            }
            other => {
                return Err(format!(
                    "{other:?} is not a wave; use \"sine\", \"square\", \"triangle\", \"saw\" or \"noise\""
                ))
            }
        };
        let edge = (i.min(n - 1 - i) as f64 / fade.max(1) as f64).min(1.0);
        out.push((v * volume * edge) as f32);
    }
    Ok(out)
}

/// `sound.tone(frequency, seconds, wave = "sine", volume = 0.5)`: a sound
/// made from a wave, for beeps, alarms and retro effects.
pub fn tone(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let number = |i: usize, what: &str, default: Option<f64>| -> VmResult<f64> {
        match args.get(i) {
            Some(Value::Int(v)) => Ok(*v as f64),
            Some(Value::Float(v)) => Ok(*v),
            None | Some(Value::Missing) if default.is_some() => Ok(default.unwrap_or(0.0)),
            _ => Err(type_error(format!("{what} must be a number"))),
        }
    };
    let frequency = number(0, "the frequency", None)?;
    let seconds = number(1, "the length in seconds", Some(0.2))?;
    let wave = match args.get(2) {
        Some(Value::Str(s)) => s.to_string(),
        _ => "sine".to_string(),
    };
    let volume = number(3, "the volume", Some(0.5))?;
    let samples =
        tone_samples(frequency, seconds, &wave, volume).map_err(|e| err("ValueError", e))?;
    #[cfg(feature = "sound")]
    {
        Ok(imp::wrap(imp::Sound {
            channels: 1,
            rate: 44100,
            samples: samples.into(),
        }))
    }
    #[cfg(not(feature = "sound"))]
    {
        let _ = samples;
        Err(unavailable())
    }
}

/// `sound.available()`: whether this computer has a sound device.
pub fn available(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    #[cfg(feature = "sound")]
    {
        Ok(Value::Bool(imp::available()))
    }
    #[cfg(not(feature = "sound"))]
    {
        Ok(Value::Bool(false))
    }
}

/// `sound.stop_all()`: silence.
pub fn stop_all(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    #[cfg(feature = "sound")]
    imp::stop_all();
    Ok(Value::None)
}
