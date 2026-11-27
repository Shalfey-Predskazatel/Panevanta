use crate::win;
use windows::{
    Graphics::SizeInt32,
    Win32::{
        Foundation::POINT,
        Graphics::{Direct3D11::*, Dxgi::Common::*},
        UI::WindowsAndMessaging::GetCursorPos,
    },
};

pub fn multiplier(input: i32, hz: f64) -> i32 {
    ((hz / input as f64 + 0.012).floor() as i32).clamp(1, 8)
}

#[cfg(test)]
mod tests {
    use super::multiplier;

    #[test]
    fn picks_the_highest_supported_factor_under_refresh_rate() {
        assert_eq!(multiplier(24, 180.0), 7);
        assert_eq!(multiplier(60, 179.82), 3);
        assert_eq!(multiplier(120, 60.0), 1);
        assert_eq!(multiplier(24, 360.0), 8);
    }
}

pub struct Probe {
    context: ID3D11DeviceContext,
    staging: ID3D11Texture2D,
    area: D3D11_BOX,
    pixels: Vec<u16>,
    mouse: POINT,
    mouse_at: i64,
    start: i64,
    last_change: i64,
    changes: u32,
    intervals: Vec<f64>,
    candidate: i32,
    repeats: u8,
    pub observed: f64,
}

impl Probe {
    pub fn new(texture: &ID3D11Texture2D, size: SizeInt32) -> windows::core::Result<Self> {
        let width = (size.Width.min(192) & !1) as u32;
        let height = (size.Height.min(144) & !1) as u32;
        let left = (size.Width as u32 - width) / 2;
        let top = (size.Height as u32 - height) / 2;
        let mut staging = None;
        let context = unsafe {
            let device = texture.GetDevice()?;
            device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_R16G16B16A16_FLOAT,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut staging),
            )?;
            device.GetImmediateContext()?
        };
        Ok(Self {
            context,
            staging: staging.expect("D3D11 staging texture"),
            area: D3D11_BOX {
                left,
                top,
                front: 0,
                right: left + width,
                bottom: top + height,
                back: 1,
            },
            pixels: vec![0; (width * height / 4) as usize],
            mouse: cursor(),
            mouse_at: 0,
            start: 0,
            last_change: 0,
            changes: 0,
            intervals: Vec::with_capacity(240),
            candidate: 0,
            repeats: 0,
            observed: 0.,
        })
    }

    pub fn measure(
        &mut self,
        texture: &ID3D11Texture2D,
        time: i64,
    ) -> windows::core::Result<Option<i32>> {
        let mouse = cursor();
        let frequency = win::frequency();
        if mouse != self.mouse {
            self.mouse = mouse;
            self.mouse_at = time;
            self.repeats = 0;
        }
        if time - self.mouse_at < frequency * 5 / 4 {
            self.start = 0;
            self.last_change = 0;
            self.changes = 0;
            self.intervals.clear();
            return Ok(None);
        }

        let difference = self.difference(texture)?;
        if difference >= 0.004 {
            self.changes += 1;
            if self.last_change != 0 && time > self.last_change {
                self.intervals
                    .push((time - self.last_change) as f64 / frequency as f64);
            }
            self.last_change = time;
        }
        if self.start == 0 {
            self.start = time;
        }
        let elapsed = (time - self.start) as f64 / frequency as f64;
        if elapsed < 2. {
            return Ok(None);
        }

        let mut measured = self.changes as f64 / elapsed;
        if self.intervals.len() >= 12 {
            self.intervals.sort_unstable_by(f64::total_cmp);
            let typical = 1. / self.intervals[self.intervals.len() / 2];
            if measured >= 35. && typical > measured * 1.10 && typical < measured * 1.25 {
                measured = typical;
            }
        }
        self.changes = 0;
        self.start = time;
        self.intervals.clear();
        self.observed = measured;
        const RATES: [i32; 14] = [24, 25, 30, 48, 50, 60, 72, 75, 90, 100, 120, 144, 165, 180];
        let best = RATES
            .into_iter()
            .min_by(|a, b| {
                (*a as f64 - measured)
                    .abs()
                    .total_cmp(&(*b as f64 - measured).abs())
            })
            .unwrap();
        if (best as f64 - measured).abs() / best as f64 > 0.085 {
            self.repeats = 0;
            return Ok(None);
        }
        self.repeats = if self.candidate == best {
            self.repeats + 1
        } else {
            1
        };
        self.candidate = best;
        Ok((self.repeats >= 2).then_some(best))
    }

    fn difference(&mut self, texture: &ID3D11Texture2D) -> windows::core::Result<f32> {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context.CopySubresourceRegion(
                &self.staging,
                0,
                0,
                0,
                0,
                texture,
                0,
                Some(&self.area),
            );
            self.context
                .Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let data = mapped.pData.cast::<u8>();
            let width = (self.area.right - self.area.left) as usize;
            let height = (self.area.bottom - self.area.top) as usize;
            let mut changed = 0;
            let mut index = 0;
            for y in (0..height).step_by(2) {
                let row = data.add(y * mapped.RowPitch as usize).cast::<u16>();
                for x in (0..width).step_by(2) {
                    let value = *row.add(x * 4 + 1);
                    changed += usize::from(value.abs_diff(self.pixels[index]) > 10);
                    self.pixels[index] = value;
                    index += 1;
                }
            }
            self.context.Unmap(&self.staging, 0);
            Ok(changed as f32 / index as f32)
        }
    }
}

fn cursor() -> POINT {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    point
}
