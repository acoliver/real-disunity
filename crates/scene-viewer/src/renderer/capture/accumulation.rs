use crate::{CaptureOutput, RetainedCaptureProducts, RetainedCaptureRecipe};
use worldrunner_render_scene::{
    Viewport,
    offscreen::{LinearRgbaImage, RgbaImage},
};

pub(super) struct Accumulation {
    size: Viewport,
    factor: u32,
    divisor: u64,
    display: Option<Vec<[u64; 4]>>,
    linear: Option<Vec<[f64; 4]>>,
}
impl Accumulation {
    pub fn new(size: Viewport, recipe: &RetainedCaptureRecipe) -> Self {
        let count = usize::try_from(u64::from(size.width) * u64::from(size.height))
            .expect("admitted pixels");
        Self {
            size,
            factor: recipe.supersample(),
            divisor: u64::from(recipe.supersample()).pow(2) * u64::from(recipe.temporal_samples()),
            display: recipe
                .outputs()
                .contains(&CaptureOutput::Srgb8)
                .then(|| vec![[0; 4]; count]),
            linear: recipe
                .outputs()
                .contains(&CaptureOutput::LinearHdr)
                .then(|| vec![[0.0; 4]; count]),
        }
    }
    pub fn add(&mut self, products: &RetainedCaptureProducts) {
        let width = self.size.width * self.factor;
        if let Some(image) = &products.srgb8 {
            assert_eq!(
                image.viewport,
                Viewport {
                    width,
                    height: self.size.height * self.factor
                }
            );
        }
        if let Some(image) = &products.linear_hdr {
            assert_eq!(
                image.viewport,
                Viewport {
                    width,
                    height: self.size.height * self.factor
                }
            );
        }
        for y in 0..self.size.height {
            for x in 0..self.size.width {
                let output = (y * self.size.width + x) as usize;
                for dy in 0..self.factor {
                    for dx in 0..self.factor {
                        let input =
                            ((y * self.factor + dy) * width + x * self.factor + dx) as usize;
                        self.add_pixel(products, output, input);
                    }
                }
            }
        }
    }
    fn add_pixel(&mut self, products: &RetainedCaptureProducts, output: usize, input: usize) {
        if let Some(sum) = &mut self.display {
            let image = products.srgb8.as_ref().expect("requested display");
            for (channel, value) in sum[output].iter_mut().enumerate() {
                *value += u64::from(image.pixels[input * 4 + channel]);
            }
        }
        if let Some(sum) = &mut self.linear {
            let image = products.linear_hdr.as_ref().expect("requested linear");
            for (channel, value) in sum[output].iter_mut().enumerate() {
                *value += f64::from(image.pixels[input][channel]);
            }
        }
    }
    pub fn finish(self) -> RetainedCaptureProducts {
        RetainedCaptureProducts {
            srgb8: self.display.map(|pixels| RgbaImage {
                viewport: self.size,
                pixels: pixels
                    .into_iter()
                    .flat_map(|pixel| {
                        pixel.map(|v| {
                            u8::try_from((v + self.divisor / 2) / self.divisor)
                                .expect("averaged byte")
                        })
                    })
                    .collect(),
            }),
            linear_hdr: self.linear.map(|pixels| LinearRgbaImage {
                viewport: self.size,
                pixels: pixels
                    .into_iter()
                    .map(|pixel| {
                        (glam::DVec4::from_array(pixel) / divisor_f64(self.divisor))
                            .as_vec4()
                            .to_array()
                    })
                    .collect(),
            }),
        }
    }
}
fn divisor_f64(value: u64) -> f64 {
    // Admission bounds raster dimensions; temporal count is a u32.
    f64::from(u32::try_from(value >> 32).expect("upper word")) * 4_294_967_296.0
        + f64::from(u32::try_from(value & 0xffff_ffff).expect("lower word"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_integer_box_filter_and_temporal_average() {
        let recipe = RetainedCaptureRecipe::new(
            2,
            2,
            crate::CaptureJitter::None,
            false,
            vec![CaptureOutput::Srgb8],
            10000,
        )
        .unwrap();
        let mut sum = Accumulation::new(
            Viewport {
                width: 1,
                height: 1,
            },
            &recipe,
        );
        for value in [0, 8] {
            sum.add(&RetainedCaptureProducts {
                srgb8: Some(RgbaImage {
                    viewport: Viewport {
                        width: 2,
                        height: 2,
                    },
                    pixels: vec![
                        value,
                        1,
                        2,
                        255,
                        value + 2,
                        3,
                        4,
                        255,
                        value + 4,
                        5,
                        6,
                        255,
                        value + 6,
                        7,
                        8,
                        255,
                    ],
                }),
                linear_hdr: None,
            });
        }
        assert_eq!(sum.finish().srgb8.unwrap().pixels, [7, 4, 5, 255]);
    }
}
