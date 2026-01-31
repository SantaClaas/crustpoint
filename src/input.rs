//! Reads analog values from GPIO pins. These values are used to determine the state of buttons and battery level.

use defmt::info;
use esp_hal::{
    Async,
    analog::adc::{Adc, AdcCalLine, AdcConfig, AdcPin, Attenuation},
    peripherals::{ADC1, GPIO0, GPIO1, GPIO2},
};

/// Measured values and rough midway points
/// Midway points:     ~2850 ~2300 ~1550 ~550
/// Recorded values: 3087, 2629, 2013, 1117, 4
const PIN_1_RANGES: [u16; 5] = [2850, 2300, 1550, 550, 0];

enum Pin {
    One,
    Two,
}

/// Measured values and rough midway points
/// Midway points:               ~2350  ~850
/// Recorded values:            3087, 1670, 4
const PIN_2_RANGES: [u16; 3] = [2350, 850, 0];
fn get_active_button(pin_value: u16, ranges: &[u16], pin: Pin) -> Option<u8> {
    let number_of_buttons: u8 = match pin {
        Pin::One => 4,
        Pin::Two => 2,
    };

    for button_number in 0..number_of_buttons {
        let start = ranges[usize::from(button_number) + 1];
        let end = ranges[usize::from(button_number)];
        // if (start..end).contains(&pin_value) {
        if start < pin_value && pin_value <= end {
            return Some(button_number);
        }
    }

    None
}

pub(crate) struct Analog<'a> {
    adc: Adc<'a, ADC1<'a>, Async>,
    pin: (
        AdcPin<GPIO0<'a>, ADC1<'a>, AdcCalLine<ADC1<'a>>>,
        AdcPin<GPIO1<'a>, ADC1<'a>, AdcCalLine<ADC1<'a>>>,
        AdcPin<GPIO2<'a>, ADC1<'a>, AdcCalLine<ADC1<'a>>>,
    ),
}

impl<'a> Analog<'a> {
    pub(crate) fn new(adc: ADC1<'a>, pin_0: GPIO0<'a>, pin_1: GPIO1<'a>, pin_2: GPIO2<'a>) -> Self {
        let mut configuration = AdcConfig::new();
        let pin_0 = configuration
            .enable_pin_with_cal::<_, AdcCalLine<ADC1<'static>>>(pin_0, Attenuation::_11dB);
        let pin_1 = configuration
            .enable_pin_with_cal::<_, AdcCalLine<ADC1<'static>>>(pin_1, Attenuation::_11dB);
        let pin_2 = configuration
            .enable_pin_with_cal::<_, AdcCalLine<ADC1<'static>>>(pin_2, Attenuation::_11dB);
        let adc = Adc::new(adc, configuration).into_async();

        Self {
            adc,
            pin: (pin_0, pin_1, pin_2),
        }
    }

    async fn read_values(&mut self) -> (u16, u16, u16) {
        let value_1 = self.adc.read_oneshot(&mut self.pin.0).await;
        let value_2 = self.adc.read_oneshot(&mut self.pin.1).await;
        let value_3 = self.adc.read_oneshot(&mut self.pin.2).await;
        (value_1, value_2, value_3)
    }

    pub(crate) async fn poll(&mut self) {
        let values = self.read_values().await;
        info!("Battery? {}", values.0);
        let button_1 = get_active_button(values.1, &PIN_1_RANGES, Pin::One);
        let button_2 = get_active_button(values.2, &PIN_2_RANGES, Pin::Two);
        match (button_1, button_2) {
            (Some(button_1), Some(button_2)) => {
                info!("Button 1: {}, Button 2: {}", button_1, button_2);
            }
            (Some(button_1), None) => {
                info!("Button 1: {}", button_1);
            }
            (None, Some(button_2)) => {
                info!("Button 2: {}", button_2);
            }
            (None, None) => {
                info!("No button pressed");
            }
        }
    }
}

impl<'a> Future for Analog<'a> {
    type Output = ();

    fn poll(
        self: core::pin::Pin<&mut Self>,
        context: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        todo!()
    }
}
