use clap::Parser;
use egui::Color32;
use egui::FontData;
use egui::FontDefinitions;
use egui::FontFamily;
use egui::Label;
use egui::Sense;
use egui::X11WindowType::Dock;
use log::{debug, error, info, log_enabled, Level};
use std::io::Read;
use std::process::Command;
use std::time::Duration;

static DEFAULT_HEIGHT: u16 = 500;
static DEFAULT_WIDTH: u16 = 800;
static DEFAULT_MARGIN: f32 = 20.0;
static DEFAULT_ROUNDING: f32 = 32.0;
static DEFAULT_BACKGROUND_COLOR: &str = "#666699";

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    width: Option<u16>,
    #[arg(long)]
    height: Option<u16>,
    #[arg(long)]
    position: Option<String>,
    #[arg(long)]
    background_color: Option<String>,
    #[arg(long)]
    text_color: Option<String>,
}

struct Vju {
    buffer: String,
    output: String,
}

fn platform() -> Option<String> {
    let mut platform: Option<String> = None;

    let output = Command::new("uname")
        .output()
        .expect("Failed to execute process");

    platform = Some(String::from_utf8_lossy(&output.stdout).to_string());
    return platform;
}

fn screen_resolution() -> (Option<u16>, Option<u16>) {
    let mut x: Option<u16> = None;
    let mut y: Option<u16> = None;

    let output = Command::new("osascript")
        .args([
            "-e",
            "tell application \"Finder\" to get bounds of window of desktop",
        ])
        .output()
        .expect("Failed to execute process");

    let binding = String::from_utf8_lossy(&output.stdout);
    let toks = binding.split(",");
    let mut n = 0;
    for tok in toks {
        if n == 2 {
            x = Some(
                tok.trim()
                    .replace("\n", "")
                    .clone()
                    .parse::<u16>()
                    .expect("Failed to parse x resolution as int"),
            );
        } else if n == 3 {
            y = Some(
                tok.trim()
                    .replace("\n", "")
                    .clone()
                    .parse::<u16>()
                    .expect("Failed to parse y resolution as int"),
            );
        }
        n += 1
    }

    return (y, x);
}

fn vju_position(x: u16, y: u16, width: u16, height: u16) -> (f32, f32) {
    let x_pos: f32;
    let y_pos: f32;

    x_pos = (x - width / 2) as f32;
    y_pos = (y - height / 2) as f32;
    log::info!("{:?} {:?}", x_pos, y_pos);
    return (400.0, 200.0);
    return (x_pos, y_pos);
}

fn add_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // Quivira
    fonts.font_data.insert(
        "custom_font".to_owned(),
        FontData::from_static(include_bytes!("/Library/Fonts/Quivira.ttf")),
    );

    // Put font as fallback
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .push("custom_font".to_owned());

    ctx.set_fonts(fonts);
}

impl Default for Vju {
    fn default() -> Self {
        let buffer = String::new();
        let output = String::new();
        Self { output, buffer }
    }
}

impl eframe::App for Vju {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_secs(1));
        add_fonts(ctx);

        let mut text_colour: egui::Color32 = Default::default();
        let vju_frame = egui::containers::Frame {
              inner_margin: egui::Margin { left: DEFAULT_MARGIN, right: DEFAULT_MARGIN, top: DEFAULT_MARGIN, bottom: DEFAULT_MARGIN },
              fill: Color32::from_hex(&DEFAULT_BACKGROUND_COLOR).unwrap(),
              rounding: egui::Rounding { nw: DEFAULT_ROUNDING, ne: DEFAULT_ROUNDING, sw: DEFAULT_ROUNDING, se: DEFAULT_ROUNDING },
              ..Default::default()
          };

        egui::CentralPanel::default().frame(vju_frame).show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                let bufsize = std::io::stdin()
                    .read_to_string(&mut self.buffer)
                    .unwrap()
                    .clone();
                if bufsize > 0 {
                    log::info!("Read {:?} bytes from stdin", bufsize);
                    self.output = self.buffer.clone();
                }
                if ui
                    .add(Label::new(egui::RichText::new(self.output.clone())).sense(Sense::click()))
                    .clicked()
                {
                    log::info!("Label clicked");
                }
            });
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    let args = Args::parse();

    // Log to stderr if you run with `RUST_LOG=debug`)
    env_logger::init();

    let platform: String = platform().unwrap().trim().to_string();
    log::info!("Running on: {}", platform);

    let height: u16;
    let width: u16;

    // Set defaults for widget size
    if args.height.is_none() {
        height = DEFAULT_HEIGHT;
    } else {
        height = args.height.unwrap();
    }
    if args.width.is_none() {
        width = DEFAULT_WIDTH;
    } else {
        width = args.height.unwrap();
    }

    // Position vju
    let (res_x, res_y) = screen_resolution();
    let (pos_x, pos_y) = vju_position(res_x.unwrap(), res_y.unwrap(), width, height);

    log::info!("Resolution: {:?}x{:?}", res_x.unwrap(), res_y.unwrap());
    log::info!("Positioning at: {:?}x{:?}", pos_x, pos_y);
    log::info!("vju size: {}x{}", height, width);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_position([pos_x, pos_y])
            .with_inner_size([width as f32, height as f32])
            .with_taskbar(false)
            .with_window_type(Dock)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native("vju", options, Box::new(|_cc| Box::<Vju>::default()))
}
