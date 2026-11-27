use serde::Deserialize;
use std::{f32::consts::TAU, ffi::c_void, sync::OnceLock};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cue {
    Select,
    Refresh,
    Settings,
    Theme,
    EffectOn,
    EffectOff,
    SoundOn,
    SoundOff,
    Ready,
    Stop,
    Error,
    Hover,
    Start,
    Stream,
    Minimize,
    Maximize,
    Tray,
    Journal,
}

impl Cue {
    fn notes(self) -> &'static [(f32, u32, u32)] {
        match self {
            Self::Select => &[(740., 0, 64)],
            Self::Refresh => &[(659., 0, 82), (988., 65, 113)],
            Self::Settings => &[(587., 0, 90), (784., 65, 120)],
            Self::Theme => &[(740., 0, 77), (932., 70, 110), (1245., 145, 121)],
            Self::EffectOn => &[(554., 0, 76), (831., 68, 116)],
            Self::EffectOff => &[(740., 0, 77), (494., 68, 118)],
            Self::SoundOn => &[(698., 0, 84), (880., 68, 96), (1047., 142, 122)],
            Self::SoundOff => &[(880., 0, 80), (659., 64, 115)],
            Self::Ready => &[(523., 0, 104), (698., 82, 116), (1047., 170, 153)],
            Self::Stop => &[(659., 0, 94), (494., 77, 122)],
            Self::Error => &[(587., 0, 111), (466., 91, 128), (370., 190, 154)],
            Self::Hover => &[(587., 0, 47)],
            Self::Start => &[(440., 0, 75), (659., 59, 110)],
            Self::Stream => &[(392., 0, 89), (588., 70, 89), (784., 139, 112)],
            Self::Minimize => &[(592., 0, 62), (445., 47, 93)],
            Self::Maximize => &[(466., 0, 70), (622., 54, 94)],
            Self::Tray => &[(660., 0, 61), (528., 43, 82), (396., 102, 102)],
            Self::Journal => &[(523., 0, 57), (786., 48, 79)],
        }
    }

    pub fn play(self, enabled: bool) -> bool {
        if !enabled {
            return false;
        }

        #[link(name = "winmm")]
        unsafe extern "system" {
            fn PlaySoundW(sound: *const c_void, module: *mut c_void, flags: u32) -> i32;
        }
        const ASYNC_MEMORY_NODEFAULT: u32 = 0x0001 | 0x0002 | 0x0004;
        const SND_NOSTOP: u32 = 0x0010;
        let hover = matches!(self, Self::Hover);
        static WAVES: [OnceLock<Vec<u8>>; Cue::Journal as usize + 1] =
            [const { OnceLock::new() }; Cue::Journal as usize + 1];
        // PlaySound reads the memory asynchronously. Each immutable waveform lives until process exit.
        let wave = WAVES[self as usize]
            .get_or_init(|| waveform(self.notes(), if hover { 0.015 } else { 0.045 }));
        unsafe {
            PlaySoundW(
                wave.as_ptr().cast(),
                std::ptr::null_mut(),
                ASYNC_MEMORY_NODEFAULT | if hover { SND_NOSTOP } else { 0 },
            ) != 0
        }
    }
}

fn waveform(notes: &[(f32, u32, u32)], level: f32) -> Vec<u8> {
    const RATE: u32 = 22_050;
    let at = |ms| (ms * RATE / 1000) as usize;
    let frames = at(notes
        .iter()
        .map(|&(_, start, len)| start + len)
        .max()
        .unwrap());
    let mut pcm = vec![0f32; frames];
    for &(frequency, start, length) in notes {
        let count = at(length);
        let attack = at(12) as f32;
        for (position, output) in pcm[at(start)..at(start) + count].iter_mut().enumerate() {
            let frame = position as f32;
            let attack_gain = 0.5 - 0.5 * (std::f32::consts::PI * (frame / attack).min(1.)).cos();
            let decay = (1. - frame / count as f32).powi(3);
            let phase = frame * TAU * frequency / RATE as f32;
            let wave = phase.sin() + 0.105 * (phase * 2.).sin() + 0.025 * (phase * 3.).sin();
            *output += wave * attack_gain * decay * level;
        }
    }
    let bytes = (pcm.len() * 2) as u32;
    let mut result = Vec::with_capacity(bytes as usize + 44);
    result.extend(b"RIFF");
    result.extend((bytes + 36).to_le_bytes());
    result.extend(b"WAVEfmt ");
    result.extend(16u32.to_le_bytes());
    result.extend(1u16.to_le_bytes());
    result.extend(1u16.to_le_bytes());
    result.extend(RATE.to_le_bytes());
    result.extend((RATE * 2).to_le_bytes());
    result.extend(2u16.to_le_bytes());
    result.extend(16u16.to_le_bytes());
    result.extend(b"data");
    result.extend(bytes.to_le_bytes());
    for sample in pcm {
        result.extend(((sample * i16::MAX as f32) as i16).to_le_bytes());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_header_matches_soft_and_distinct_pcm_signals() {
        let ready = waveform(Cue::Ready.notes(), 0.045);
        let stop = waveform(Cue::Stop.notes(), 0.045);
        assert_eq!(&ready[0..4], b"RIFF");
        assert_eq!(&ready[8..12], b"WAVE");
        let length = u32::from_le_bytes(ready[40..44].try_into().unwrap());
        assert_eq!(length as usize, ready[44..].len());
        assert!(
            ready[44..]
                .chunks_exact(2)
                .all(|bytes| i16::from_le_bytes(bytes.try_into().unwrap()).unsigned_abs() < 3_500)
        );
        assert_ne!(ready, stop);
    }
}
