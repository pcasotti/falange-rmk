#![no_main]
#![no_std]

use embassy_nrf::{gpio::Pin, peripherals::TWISPI0, twim::Twim, Peri};
use embassy_time::{Duration, WithTimeout};
use embedded_graphics::{
    image::{Image, ImageRaw},
    mono_font::MonoTextStyle,
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyle, TextStyleBuilder, renderer::TextRenderer},
};
use rmk::{
    channel::{ControllerSub, CONTROLLER_CHANNEL},
    controller::{Controller, PollingController},
    event::ControllerEvent,
    macros::rmk_central,
    types::modifier::ModifierCombination,
};
use ssd1306::{I2CDisplayInterface, Ssd1306Async, mode::BufferedGraphicsModeAsync, prelude::*};

const LAYER_NAMES: [&str; 8] = ["BASE", "NAV", "SYM", "NUM", "ACC", "COM", "GAME", "GAME"];

struct Graphics<'a> {
    character_style: MonoTextStyle<'a, BinaryColor>,
    character_smaller: MonoTextStyle<'a, BinaryColor>,
    fill_style: PrimitiveStyle<BinaryColor>,
    stroke_style: PrimitiveStyle<BinaryColor>,
    centered_style: TextStyle,
    layer_center: Point,
    raw_shift: ImageRaw<'a, BinaryColor>,
    raw_ctrl: ImageRaw<'a, BinaryColor>,
    raw_alt: ImageRaw<'a, BinaryColor>,
    raw_gui: ImageRaw<'a, BinaryColor>,
}

impl<'a> Graphics<'a> {
    fn new(bounding_box: Rectangle) -> Self {
        let character_style = MonoTextStyle::new(
            &embedded_graphics::mono_font::ascii::FONT_9X18,
            BinaryColor::On,
        );
        let character_smaller = MonoTextStyle::new(
            &embedded_graphics::mono_font::ascii::FONT_6X10,
            BinaryColor::On,
        );
        let fill_style = PrimitiveStyle::with_fill(BinaryColor::On);
        let stroke_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
        let centered_style = TextStyleBuilder::new()
            .baseline(Baseline::Middle)
            .alignment(Alignment::Center)
            .build();
        let layer_center = Point::new(
            character_style
                .measure_string("GAME 2", Point::zero(), Baseline::Alphabetic)
                .bounding_box
                .center()
                .x,
            bounding_box.center().y,
        );
        let raw_shift = ImageRaw::<BinaryColor>::new(include_bytes!("./display/shift.raw"), 12);
        let raw_ctrl = ImageRaw::<BinaryColor>::new(include_bytes!("./display/ctrl.raw"), 12);
        let raw_alt = ImageRaw::<BinaryColor>::new(include_bytes!("./display/alt.raw"), 12);
        let raw_gui = ImageRaw::<BinaryColor>::new(include_bytes!("./display/gui.raw"), 12);

        Self {
            character_style,
            character_smaller,
            fill_style,
            stroke_style,
            centered_style,
            layer_center,
            raw_shift,
            raw_ctrl,
            raw_alt,
            raw_gui,
        }
    }
}

type Display = Ssd1306Async<
    I2CInterface<Twim<'static>>,
    DisplaySize128x32,
    BufferedGraphicsModeAsync<DisplaySize128x32>,
>;

struct DisplayConfig<SDA: Pin, SCL: Pin> {
    twim: Peri<'static, TWISPI0>,
    sda: Peri<'static, SDA>,
    scl: Peri<'static, SCL>,
}

struct DisplayController<'a, SDA: Pin, SCL: Pin> {
    sub: ControllerSub,
    config: DisplayConfig<SDA, SCL>,
    display: Option<Display>,
    layer: u8,
    modifiers: ModifierCombination,
    battery: u8,
    graphics: Graphics<'a>,
}

bind_interrupts!(struct MyIrqs {
    TWISPI0 => embassy_nrf::twim::InterruptHandler<embassy_nrf::peripherals::TWISPI0>;
});

impl<'a, SDA, SCL> DisplayController<'a, SDA, SCL>
where
    SDA: Pin,
    SCL: Pin,
{
    fn new(twim: Peri<'static, TWISPI0>, sda: Peri<'static, SDA>, scl: Peri<'static, SCL>) -> Self {
        Self {
            sub: CONTROLLER_CHANNEL.subscriber().unwrap(),
            config: DisplayConfig { twim, sda, scl },
            display: None,
            layer: 0,
            modifiers: ModifierCombination::new(),
            battery: 0,
            graphics: Graphics::new(Rectangle::new(Point::zero(), Size::new(128, 32))),
        }
    }

    async fn draw(&mut self, display: &mut Display) -> Result<(), <Display as DrawTarget>::Error> {
        display.clear_buffer();

        if self.layer > 0 {
            Text::with_text_style(
                LAYER_NAMES[self.layer as usize - 1],
                self.graphics.layer_center
                    - Point::new(
                        0,
                        self.graphics.character_style.font.character_size.height as i32 / 3 * 2,
                    ),
                self.graphics.character_smaller,
                self.graphics.centered_style,
            )
            .draw(display)?;
        }

        if self.layer < LAYER_NAMES.len() as u8 - 1 {
            Text::with_text_style(
                LAYER_NAMES[self.layer as usize + 1],
                self.graphics.layer_center
                    + Point::new(
                        0,
                        self.graphics.character_style.font.character_size.height as i32 / 3 * 2,
                    ),
                self.graphics.character_smaller,
                self.graphics.centered_style,
            )
            .draw(display)?;
        }

        Text::with_text_style(
            LAYER_NAMES[self.layer as usize],
            self.graphics.layer_center,
            self.graphics.character_style,
            self.graphics.centered_style,
        )
        .draw(display)?;

        Image::with_center(
            &self.graphics.raw_shift,
            display.bounding_box().center() + Point::new(0, 0),
        )
        .translate(Point::new(
            0,
            (self.modifiers.left_shift() || self.modifiers.right_shift()) as i32 * -4,
        ))
        .draw(display)?;
        if self.modifiers.left_shift() || self.modifiers.right_shift() {
            Rectangle::with_center(
                display.bounding_box().center() + Point::new(0, 5),
                Size::new(12, 2),
            )
            .into_styled(self.graphics.fill_style)
            .draw(display)?;
        }
        Image::with_center(
            &self.graphics.raw_ctrl,
            display.bounding_box().center() + Point::new(15, 0),
        )
        .translate(Point::new(
            0,
            (self.modifiers.left_ctrl() || self.modifiers.right_ctrl()) as i32 * -4,
        ))
        .draw(display)?;
        if self.modifiers.left_ctrl() || self.modifiers.right_ctrl() {
            Rectangle::with_center(
                display.bounding_box().center() + Point::new(15, 5),
                Size::new(12, 2),
            )
            .into_styled(self.graphics.fill_style)
            .draw(display)?;
        }
        Image::with_center(
            &self.graphics.raw_alt,
            display.bounding_box().center() + Point::new(30, 0),
        )
        .translate(Point::new(
            0,
            (self.modifiers.left_alt() || self.modifiers.right_alt()) as i32 * -4,
        ))
        .draw(display)?;
        if self.modifiers.left_alt() || self.modifiers.right_alt() {
            Rectangle::with_center(
                display.bounding_box().center() + Point::new(30, 5),
                Size::new(12, 2),
            )
            .into_styled(self.graphics.fill_style)
            .draw(display)?;
        }
        Image::with_center(
            &self.graphics.raw_gui,
            display.bounding_box().center() + Point::new(45, 0),
        )
        .translate(Point::new(
            0,
            (self.modifiers.left_gui() || self.modifiers.right_gui()) as i32 * -4,
        ))
        .draw(display)?;
        if self.modifiers.left_gui() || self.modifiers.right_gui() {
            Rectangle::with_center(
                display.bounding_box().center() + Point::new(45, 5),
                Size::new(12, 2),
            )
            .into_styled(self.graphics.fill_style)
            .draw(display)?;
        }

        Rectangle::with_corners(Point::new(123, 0), Point::new(127, 31))
            .into_styled(self.graphics.stroke_style)
            .draw(display)?;

        Rectangle::with_corners(
            Point::new(124, 32 - (self.battery as i32 * 32) / 100),
            Point::new(126, 31),
        )
        .into_styled(self.graphics.fill_style)
        .draw(display)?;

        display.flush().await
    }
}

impl<'a, SDA: Pin, SCL: Pin> Controller for DisplayController<'a, SDA, SCL> {
    type Event = ControllerEvent;

    async fn process_event(&mut self, event: Self::Event) {
        match event {
            ControllerEvent::Layer(layer) => {
                self.layer = layer;
            }
            ControllerEvent::Modifier(modifiers) => {
                self.modifiers = modifiers;
            }
            ControllerEvent::Battery(battery) => {
                self.battery = battery;
            }
            _ => (),
        }

        if self.sub.len() < 2 {
            self.update().await;
        }
    }

    async fn next_message(&mut self) -> Self::Event {
        self.sub.next_message_pure().await
    }
}

impl<'a, SDA: Pin, SCL: Pin> PollingController for DisplayController<'a, SDA, SCL> {
    const INTERVAL: embassy_time::Duration = embassy_time::Duration::from_hz(30);

    async fn update(&mut self) {
        match self.display.take() {
            Some(mut display) => {
                if let Ok(Ok(_)) = self.draw(&mut display).with_timeout(Duration::from_millis(100)).await {
                    self.display = Some(display);
                }
            }
            None => {
                let i2c = unsafe {
                    Twim::new(
                        self.config.twim.clone_unchecked(),
                        MyIrqs,
                        self.config.sda.clone_unchecked(),
                        self.config.scl.clone_unchecked(),
                        Default::default(),
                        &mut [],
                    )
                };
                let interface = I2CDisplayInterface::new(i2c);
                let mut display =
                    Ssd1306Async::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
                        .into_buffered_graphics_mode();
                if let Ok(Ok(_)) = display
                    .init()
                    .with_timeout(Duration::from_secs(1))
                    .await {
                    self.display = Some(display);
                }
            }
        }
    }
}

#[rmk_central]
mod keyboard_central {
    #[controller(poll)]
    fn display_controller() -> DisplayController {
        DisplayController::new(p.TWISPI0, p.P0_17, p.P0_20)
    }
}
