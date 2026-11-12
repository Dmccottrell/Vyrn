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

/// Where the listener is: position, and which way they face (degrees of
/// yaw, as a `vyrn.draw` camera turns).
#[derive(Clone, Copy)]
pub struct Listener {
    pub at: [f64; 3],
    pub yaw: f64,
}

thread_local! {
    static LISTENER: std::cell::Cell<Listener> = const {
        std::cell::Cell::new(Listener { at: [0.0; 3], yaw: 0.0 })
    };
}

/// Left and right volume for a sound at `at`, heard by `ear`: quieter with
/// distance (silent at `range`), and louder in the ear it is nearer.
pub fn spatial_gains(ear: Listener, at: [f64; 3], range: f64) -> [f64; 2] {
    let to = [at[0] - ear.at[0], at[1] - ear.at[1], at[2] - ear.at[2]];
    let dist = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
    let fade = (1.0 - dist / range.max(0.001)).clamp(0.0, 1.0);
    // Full volume within a metre, then falling off, then fading to silence.
    let level = fade / (1.0 + 0.3 * (dist - 1.0).max(0.0));
    if dist < 1e-6 {
        return [level, level];
    }
    // Facing +z at yaw 0, with +x to the right, as the camera.
    let (s, c) = ear.yaw.to_radians().sin_cos();
    let right = [c, 0.0, -s];
    let pan = ((to[0] * right[0] + to[2] * right[2]) / dist).clamp(-1.0, 1.0);
    // Equal power: centred is full volume in both ears.
    let angle = (pan + 1.0) * std::f64::consts::FRAC_PI_4;
    let k = std::f64::consts::SQRT_2;
    [
        (level * angle.cos() * k).min(1.0),
        (level * angle.sin() * k).min(1.0),
    ]
}

pub fn listener() -> Listener {
    LISTENER.with(std::cell::Cell::get)
}

/// `sound.listener(x, y, z, yaw = 0)` or `sound.listener(camera)`: where
/// the player hears from. Sounds played `at` a place pan and fade from here.
pub fn set_listener(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let number = |v: &Value, what: &str| -> VmResult<f64> {
        match v {
            Value::Int(i) => Ok(*i as f64),
            Value::Float(f) => Ok(*f),
            other => Err(type_error(format!(
                "{what} must be a number, not {}",
                other.type_name()
            ))),
        }
    };
    let ear = match args.first() {
        Some(Value::Foreign(f)) => {
            let pos = super::ops::collect(&f.get_attr("position")?)?;
            let [x, y, z] = pos.as_slice() else {
                return Err(type_error("`sound.listener` takes a camera, or x, y, z"));
            };
            Listener {
                at: [number(x, "x")?, number(y, "y")?, number(z, "z")?],
                yaw: number(&f.get_attr("yaw")?, "yaw")?,
            }
        }
        _ => {
            let get = |i: usize, what: &str| -> VmResult<f64> {
                match args.get(i) {
                    Some(Value::None) | Some(Value::Missing) | None => Ok(0.0),
                    Some(v) => number(v, what),
                }
            };
            Listener {
                at: [get(0, "x")?, get(1, "y")?, get(2, "z")?],
                yaw: get(3, "yaw")?,
            }
        }
    };
    let _ = ctx;
    LISTENER.with(|l| l.set(ear));
    #[cfg(feature = "sound")]
    imp::update_spatial();
    Ok(Value::None)
}

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
        /// Sounds playing at a place, to pan again when the listener moves.
        static SPATIAL: RefCell<Vec<std::rc::Weak<Spatial>>> = const { RefCell::new(Vec::new()) };
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
                    // `at = [x, y, z]`: a sound in the world, heard from
                    // `sound.listener`; `range`: how far it carries.
                    let spatial = match kw(kwargs, "at").or(args.get(3)) {
                        None | Some(Value::None) => None,
                        Some(v) => {
                            let p = crate::vm::ops::collect(v)?;
                            let [x, y, z] = p.as_slice() else {
                                return Err(type_error("`at` is [x, y, z]"));
                            };
                            let range = match kw(kwargs, "range").or(args.get(4)) {
                                Some(v) => num(v, "the range")?,
                                None => 30.0,
                            };
                            let state = Rc::new(Spatial {
                                at: std::cell::Cell::new([
                                    num(x, "x")?,
                                    num(y, "y")?,
                                    num(z, "z")?,
                                ]),
                                range,
                                gains: Default::default(),
                            });
                            state.update();
                            SPATIAL.with(|l| l.borrow_mut().push(Rc::downgrade(&state)));
                            Some(state)
                        }
                    };
                    let player = with_device(|device| {
                        let device = device?;
                        let player = rodio::Player::connect_new(device.mixer());
                        match &spatial {
                            Some(state) if looping => {
                                player.append(panned(
                                    self.buffer().repeat_infinite(),
                                    state.gains.clone(),
                                ));
                            }
                            Some(state) => {
                                player.append(panned(self.buffer(), state.gains.clone()))
                            }
                            None if looping => player.append(self.buffer().repeat_infinite()),
                            None => player.append(self.buffer()),
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
                    Ok(Value::Foreign(Rc::new(Box::new(Playing {
                        player,
                        spatial,
                    }))))
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

    /// Where a playing sound is, and the ear volumes the audio thread reads.
    pub struct Spatial {
        at: std::cell::Cell<[f64; 3]>,
        range: f64,
        gains: std::sync::Arc<std::sync::Mutex<[f32; 2]>>,
    }

    impl Spatial {
        fn update(&self) {
            let g = super::spatial_gains(super::listener(), self.at.get(), self.range);
            if let Ok(mut gains) = self.gains.lock() {
                *gains = [g[0] as f32, g[1] as f32];
            }
        }

        fn balance(&self) -> [f32; 2] {
            self.gains.lock().map_or([0.0; 2], |g| *g)
        }
    }

    /// Two channels whose volumes follow `gains`, checked every 10 ms.
    fn panned<S>(
        source: S,
        gains: std::sync::Arc<std::sync::Mutex<[f32; 2]>>,
    ) -> impl Source + Send + 'static
    where
        S: Source + Send + 'static,
    {
        let g = gains.lock().map_or([0.0; 2], |g| *g);
        rodio::source::ChannelVolume::new(source, vec![g[0], g[1]]).periodic_access(
            std::time::Duration::from_millis(10),
            move |cv| {
                if let Ok(g) = gains.lock() {
                    cv.set_volume(0, g[0]);
                    cv.set_volume(1, g[1]);
                }
            },
        )
    }

    /// Pans every placed sound for the listener's new place.
    pub fn update_spatial() {
        SPATIAL.with(|list| {
            let mut list = list.borrow_mut();
            list.retain(|s| s.strong_count() > 0);
            for s in list.iter().filter_map(std::rc::Weak::upgrade) {
                s.update();
            }
        });
    }

    /// A sound that is playing (or, without a sound device, silently
    /// pretending to).
    pub struct Playing {
        player: Option<Rc<rodio::Player>>,
        spatial: Option<Rc<Spatial>>,
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
                // [left, right] volumes of a sound placed in the world.
                "balance" => Ok(match &self.spatial {
                    Some(s) => {
                        let g = s.balance();
                        let r = |x: f32| Value::Float((f64::from(x) * 1000.0).round() / 1000.0);
                        Value::list(vec![r(g[0]), r(g[1])])
                    }
                    None => Value::None,
                }),
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
            // `playing.move_to(x, y, z)`: the sound moves (a passing car).
            if name == "move_to" {
                let Some(s) = &self.spatial else {
                    return Err(type_error("`move_to` is for sounds played `at` a place"));
                };
                let p = match args {
                    [single] => crate::vm::ops::collect(single)?,
                    many => many.to_vec(),
                };
                let [x, y, z] = p.as_slice() else {
                    return Err(type_error("`move_to` takes x, y, z"));
                };
                s.at.set([num(x, "x")?, num(y, "y")?, num(z, "z")?]);
                s.update();
                return Ok(Value::None);
            }
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
