use core::ops::{Deref, Range, RangeInclusive};

use embedded_graphics::{
    Pixel,
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, OriginDimensions, Point, Size},
};

use crate::eink_display;

enum Orientation {
    Portrait,
    Landscape,
}

pub(crate) struct Frame {
    buffer: [u8; Self::BUFFER_SIZE],
    /// The orientation is an experimental idea to allow for different display orientations.
    orientation: Orientation,
}

impl Frame {
    // The display is in portrait mode by default
    const WIDTH: u16 = eink_display::DISPLAY_WIDTH;
    const HEIGHT: u16 = eink_display::DISPLAY_HEIGHT;

    /// Each bit in a byte represents a pixel (0 = off, 1 = on)
    const WIDTH_BYTES: usize = {
        // There is no div_exact yet
        assert!(
            Self::WIDTH % 8 == 0,
            "Display width must be a multiple of 8"
        );

        Self::WIDTH.strict_div(8) as usize
    };
    pub(crate) const BUFFER_SIZE: usize = Self::WIDTH_BYTES.strict_mul(Self::HEIGHT as usize);
}

impl Default for Frame {
    fn default() -> Self {
        Frame {
            buffer: [0b1111_1111; Self::BUFFER_SIZE],
            orientation: Orientation::Portrait,
        }
    }
}

impl Deref for Frame {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl OriginDimensions for Frame {
    fn size(&self) -> Size {
        Size::new(u32::from(Self::WIDTH), u32::from(Self::HEIGHT))
    }
}

#[derive(defmt::Format)]
pub(crate) enum DrawError {
    /// If more details about the error are needed at runtime, then add them
    OutOfBounds,
}

impl DrawTarget for Frame {
    type Color = BinaryColor;

    type Error = DrawError;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        const X_RANGE: Range<u16> = 0..Frame::WIDTH;
        const Y_RANGE: Range<u16> = 0..Frame::HEIGHT;

        for Pixel(point, color) in pixels {
            let x = u16::try_from(point.x).map_err(|_| DrawError::OutOfBounds)?;
            let y = u16::try_from(point.y).map_err(|_| DrawError::OutOfBounds)?;

            if !X_RANGE.contains(&x) || !Y_RANGE.contains(&y) {
                return Err(DrawError::OutOfBounds);
            }

            // Map to pixel on hardware
            let x_hardware = usize::from(y);
            // Display is inverted
            let y_hardware = usize::from(eink_display::DISPLAY_HEIGHT - x);
            // Make it zero-indexed
            let y_index = y_hardware - 1;

            let row_start = y_index * Frame::WIDTH_BYTES;
            // Locate the byte that contains the pixel. This is a floor division
            let row_pixel_index = x_hardware / 8;
            let index = row_start + row_pixel_index;
            // The remainder defines the bit index within the byte. The part that is left over from finding the pixel index in the row (x_hardware / 8)
            let bit_index = 7 - x_hardware % 8;

            self.buffer[index] = match color {
                // E-Ink dark is charged = black
                BinaryColor::Off => self.buffer[index] | (1 << bit_index),
                // E-Ink light is not charged = white
                BinaryColor::On => self.buffer[index] & !(1 << bit_index),
            };
        }
        Ok(())
    }
}
