use serde::Deserialize;
use clap::Parser;
use log;
use env_logger;
use image::GenericImageView;

#[cfg(not(target_os = "linux"))]
mod compositor_background {
    pub fn run(_image_bytes: Vec<u8>) -> Result<(), String> {
        Err("--compositor-background needs wlr-layer-shell, which is Wayland/wlroots-only (Linux); this platform can't support it".into())
    }
}

#[cfg(target_os = "linux")]
mod compositor_background {
    use std::num::NonZeroU32;
    use std::sync::Arc;

    use image::GenericImageView;
    use smithay_client_toolkit::{
        compositor::{CompositorHandler, CompositorState},
        delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
        output::{OutputHandler, OutputState},
        registry::{ProvidesRegistryState, RegistryState},
        registry_handlers,
        shell::{
            wlr_layer::{
                Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
                LayerSurfaceConfigure,
            },
            WaylandSurface,
        },
        shm::{slot::SlotPool, Shm, ShmHandler},
    };
    use wayland_client::{
        globals::registry_queue_init,
        protocol::{wl_output, wl_shm, wl_surface},
        Connection, QueueHandle,
    };

    pub fn run(image_bytes: Vec<u8>) -> Result<(), String> {
        let conn = Connection::connect_to_env().map_err(|e| {
            format!("no Wayland compositor found ({e}); --compositor-background needs a running Wayland session")
        })?;
        let (globals, mut event_queue) = registry_queue_init(&conn)
            .map_err(|e| format!("failed to query Wayland globals: {e}"))?;
        let qh = event_queue.handle();

        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|e| format!("wl_compositor unavailable: {e}"))?;
        let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| {
            format!(
                "wlr-layer-shell unavailable ({e}); --compositor-background needs a wlroots-based compositor \
                 (sway, Hyprland, river, wayfire, ...) - GNOME and KDE don't support it"
            )
        })?;
        let shm = Shm::bind(&globals, &qh).map_err(|e| format!("wl_shm unavailable: {e}"))?;

        let mut state = CompositorBackground {
            registry_state: RegistryState::new(&globals),
            output_state: OutputState::new(&globals, &qh),
            compositor,
            layer_shell,
            shm,
            image_bytes: Arc::new(image_bytes),
            surfaces: Vec::new(),
            exit: false,
        };

        log::info!("compositor-background mode: connected, waiting for outputs");
        loop {
            event_queue
                .blocking_dispatch(&mut state)
                .map_err(|e| format!("Wayland event loop error: {e}"))?;
            if state.exit {
                log::info!("compositor-background surface(s) closed by compositor, exiting");
                return Ok(());
            }
        }
    }

    struct OutputSurface {
        output: wl_output::WlOutput,
        layer: LayerSurface,
        pool: SlotPool,
        width: u32,
        height: u32,
        scale: i32,
        configured: bool,
    }

    struct CompositorBackground {
        registry_state: RegistryState,
        output_state: OutputState,
        compositor: CompositorState,
        layer_shell: LayerShell,
        shm: Shm,
        image_bytes: Arc<Vec<u8>>,
        surfaces: Vec<OutputSurface>,
        exit: bool,
    }

    impl CompositorBackground {
        fn draw(&mut self, index: usize) {
            let Some(surf) = self.surfaces.get_mut(index) else { return };
            if surf.width == 0 || surf.height == 0 {
                return;
            }
            let buf_w = (surf.width as i32 * surf.scale).max(1);
            let buf_h = (surf.height as i32 * surf.scale).max(1);
            let stride = buf_w * 4;

            let Some(rgba) = cover_scale_rgba(&self.image_bytes, buf_w as u32, buf_h as u32) else {
                log::warn!("failed to decode --compositor-background image");
                return;
            };

            let (buffer, canvas) =
                match surf.pool.create_buffer(buf_w, buf_h, stride, wl_shm::Format::Argb8888) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("failed to create compositor-background shm buffer: {e}");
                        return;
                    }
                };

            for (chunk, px) in canvas.chunks_exact_mut(4).zip(rgba.pixels()) {
                let [r, g, b, a] = px.0;
                let color = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
                chunk.copy_from_slice(&color.to_le_bytes());
            }

            let wl_surface = surf.layer.wl_surface();
            wl_surface.set_buffer_scale(surf.scale);
            wl_surface.damage_buffer(0, 0, buf_w, buf_h);
            if let Err(e) = buffer.attach_to(wl_surface) {
                log::warn!("failed to attach compositor-background buffer: {e}");
                return;
            }
            surf.layer.commit();
            surf.configured = true;
        }
    }

    impl CompositorHandler for CompositorBackground {
        fn scale_factor_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            surface: &wl_surface::WlSurface,
            new_factor: i32,
        ) {
            if let Some(index) = self.surfaces.iter().position(|s| s.layer.wl_surface() == surface) {
                self.surfaces[index].scale = new_factor.max(1);
                if self.surfaces[index].configured {
                    self.draw(index);
                }
            }
        }

        fn transform_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _new_transform: wl_output::Transform,
        ) {
        }

        fn frame(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _time: u32,
        ) {
        }

        fn surface_enter(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _output: &wl_output::WlOutput,
        ) {
        }

        fn surface_leave(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _output: &wl_output::WlOutput,
        ) {
        }
    }

    impl OutputHandler for CompositorBackground {
        fn output_state(&mut self) -> &mut OutputState {
            &mut self.output_state
        }

        fn new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
            let surface = self.compositor.create_surface(qh);
            let layer = self.layer_shell.create_layer_surface(
                qh,
                surface,
                Layer::Background,
                Some("vju-compositor-background"),
                Some(&output),
            );
            layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();

            let pool = match SlotPool::new(4, &self.shm) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("failed to create shm pool for new output: {e}");
                    return;
                }
            };

            self.surfaces.push(OutputSurface {
                output,
                layer,
                pool,
                width: 0,
                height: 0,
                scale: 1,
                configured: false,
            });
        }

        fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}

        fn output_destroyed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            output: wl_output::WlOutput,
        ) {
            self.surfaces.retain(|s| s.output != output);
            if self.surfaces.is_empty() {
                self.exit = true;
            }
        }
    }

    impl LayerShellHandler for CompositorBackground {
        fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
            self.surfaces.retain(|s| &s.layer != layer);
            if self.surfaces.is_empty() {
                self.exit = true;
            }
        }

        fn configure(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            layer: &LayerSurface,
            configure: LayerSurfaceConfigure,
            _serial: u32,
        ) {
            let Some(index) = self.surfaces.iter().position(|s| &s.layer == layer) else { return };
            let (w, h) = configure.new_size;
            self.surfaces[index].width = NonZeroU32::new(w).map_or(1920, NonZeroU32::get);
            self.surfaces[index].height = NonZeroU32::new(h).map_or(1080, NonZeroU32::get);
            self.draw(index);
        }
    }

    impl ShmHandler for CompositorBackground {
        fn shm_state(&mut self) -> &mut Shm {
            &mut self.shm
        }
    }

    impl ProvidesRegistryState for CompositorBackground {
        fn registry(&mut self) -> &mut RegistryState {
            &mut self.registry_state
        }
        registry_handlers![OutputState];
    }

    delegate_compositor!(CompositorBackground);
    delegate_output!(CompositorBackground);
    delegate_shm!(CompositorBackground);
    delegate_layer!(CompositorBackground);
    delegate_registry!(CompositorBackground);

    fn cover_scale_rgba(bytes: &[u8], target_w: u32, target_h: u32) -> Option<image::RgbaImage> {
        let img = image::load_from_memory(bytes).ok()?;
        let (orig_w, orig_h) = img.dimensions();
        if orig_w == 0 || orig_h == 0 || target_w == 0 || target_h == 0 {
            return None;
        }
        let scale = (target_w as f32 / orig_w as f32).max(target_h as f32 / orig_h as f32);
        let scaled_w = ((orig_w as f32 * scale).ceil() as u32).max(1);
        let scaled_h = ((orig_h as f32 * scale).ceil() as u32).max(1);
        let resized = img.resize_exact(scaled_w, scaled_h, image::imageops::FilterType::Lanczos3);
        let x = scaled_w.saturating_sub(target_w) / 2;
        let y = scaled_h.saturating_sub(target_h) / 2;
        Some(image::imageops::crop_imm(&resized, x, y, target_w.min(scaled_w), target_h.min(scaled_h)).to_image())
    }
}

/// Parse a hex color string (e.g. "#ff9900" or "#ff9900ff") to egui::Color32.
pub fn parse_hex_color(s: &str) -> Option<egui::Color32> {
    let s = s.trim_start_matches('#');
    match s.len() {
        6 => u32::from_str_radix(s, 16).ok().map(|rgb| {
            let r = ((rgb >> 16) & 0xFF) as u8;
            let g = ((rgb >> 8) & 0xFF) as u8;
            let b = (rgb & 0xFF) as u8;
            let a = 255u8;
            egui::Color32::from_rgba_premultiplied(r, g, b, a)
        }),
        8 => u32::from_str_radix(s, 16).ok().map(|rgba| {
            let r = ((rgba >> 24) & 0xFF) as u8;
            let g = ((rgba >> 16) & 0xFF) as u8;
            let b = ((rgba >> 8) & 0xFF) as u8;
            let a = (rgba & 0xFF) as u8;
            egui::Color32::from_rgba_premultiplied(r, g, b, a)
        }),
        _ => None,
    }
}

/// Print a line to stdout, flushing immediately (needed since stdout may be piped rather
/// than a line-buffered terminal).
fn print_flushed(s: &str) {
    println!("{}", s);
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

/// Print a line to stdout and exit immediately. Used for every "emit a signal line and quit"
/// case: `vju-exit`, `vju-read`, `vju-key-*`.
fn print_and_exit(s: &str) -> ! {
    print_flushed(s);
    std::process::exit(0);
}

/// Scale factor to fit `orig_w x orig_h` within `max_width x max_height`, preserving aspect
/// ratio and never upscaling (capped at 1.0). Shared by the raster (resize_image_to_fit) and
/// SVG (decode_svg) decode paths so the "never upscale, fit the tighter dimension" rule lives
/// in exactly one place.
fn fit_scale(orig_w: f32, orig_h: f32, max_width: u32, max_height: u32) -> f32 {
    (max_width as f32 / orig_w).min(max_height as f32 / orig_h).min(1.0)
}

/// Resize an image so it never exceeds max_width or max_height, preserving aspect ratio.
pub fn resize_image_to_fit(img: &image::DynamicImage, max_width: u32, max_height: u32) -> (image::DynamicImage, [usize; 2]) {
    let (orig_w, orig_h) = img.dimensions();
    let scale = fit_scale(orig_w as f32, orig_h as f32, max_width, max_height);
    let new_w = (orig_w as f32 * scale).round() as u32;
    let new_h = (orig_h as f32 * scale).round() as u32;
    let resized = if scale < 1.0 {
        img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3)
    } else {
        img.clone()
    };
    (resized, [new_w as usize, new_h as usize])
}

/// Decode image bytes into an egui-ready color image, scaled to fit within
/// max_width/max_height (preserving aspect ratio, never upscaling). Tries raster formats
/// (PNG, JPEG, etc. via the `image` crate) first, then falls back to SVG. Returns `None` if
/// the bytes are neither.
pub fn decode_image(bytes: &[u8], max_width: u32, max_height: u32) -> Option<(egui::ColorImage, [usize; 2])> {
    if let Ok(img) = image::load_from_memory(bytes) {
        let (resized, size) = resize_image_to_fit(&img, max_width, max_height);
        let rgba = resized.to_rgba8();
        let pixels: Vec<egui::Color32> = rgba
            .pixels()
            .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
            .collect();
        let color_image = egui::ColorImage { size, pixels, source_size: egui::Vec2::new(size[0] as f32, size[1] as f32) };
        return Some((color_image, size));
    }
    decode_svg(bytes, max_width, max_height)
}

/// Rasterizes SVG bytes directly at a size that fits within max_width/max_height (preserving
/// aspect ratio, never upscaling past the SVG's intrinsic size). Rendering directly at the
/// target size (rather than rendering at some default size and downscaling afterward) keeps
/// vector output crisp.
///
/// Built without resvg's `text` feature, so `<text>` elements inside the SVG won't render.
fn decode_svg(bytes: &[u8], max_width: u32, max_height: u32) -> Option<(egui::ColorImage, [usize; 2])> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let intrinsic = tree.size();
    let (orig_w, orig_h) = (intrinsic.width(), intrinsic.height());
    if orig_w <= 0.0 || orig_h <= 0.0 {
        return None;
    }
    let scale = fit_scale(orig_w, orig_h, max_width, max_height);
    let target_w = ((orig_w * scale).round() as u32).max(1);
    let target_h = ((orig_h * scale).round() as u32).max(1);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(target_w, target_h)?;
    let transform = resvg::tiny_skia::Transform::from_scale(target_w as f32 / orig_w, target_h as f32 / orig_h);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let size = [target_w as usize, target_h as usize];
    // tiny-skia's Pixmap is premultiplied-alpha RGBA8, same as egui::Color32's own storage
    // format, so this is a straight byte reinterpretation rather than a real conversion.
    let pixels: Vec<egui::Color32> = pixmap
        .data()
        .chunks_exact(4)
        .map(|p| egui::Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    let color_image = egui::ColorImage { size, pixels, source_size: egui::Vec2::new(target_w as f32, target_h as f32) };
    Some((color_image, size))
}

/// Find a config file path for the theme (e.g. vju-config.toml in CWD or $HOME).
pub fn find_config_path() -> Option<String> {
    let candidates = [
        "vju-config.toml".to_string(),
        std::env::var("HOME").ok().map(|h| format!("{}/.config/vju/vju-config.toml", h)).unwrap_or_default(),
    ];
    for path in &candidates {
        if !path.is_empty() && std::path::Path::new(path).exists() {
            return Some(path.clone());
        }
    }
    None
}

/// Computes indices into `items` whose text contains `query` (case-insensitive substring
/// match). An empty query matches everything, preserving original order.
pub fn filter_indices(items: &[String], query: &str) -> Vec<usize> {
    if query.is_empty() {
        (0..items.len()).collect()
    } else {
        let q = query.to_lowercase();
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }
}

/// Clamps a selection index into `[0, filtered_len)`, or 0 if `filtered_len` is 0.
pub fn clamp_selected(selected: usize, filtered_len: usize) -> usize {
    if filtered_len == 0 {
        0
    } else if selected >= filtered_len {
        filtered_len - 1
    } else {
        selected
    }
}

/// Keyboard hint text for a given mode, shown in the `--hint` footer. `image_count` only
/// matters for `Mode::Image`: navigation keys are only worth mentioning when there's more than
/// one image to navigate between.
pub fn hint_text_for_mode(mode: &Mode, image_count: usize) -> &'static str {
    match mode {
        Mode::Select => "↑/k up  ↓/j down  Enter select  /  search  Esc/q quit",
        Mode::View => "Esc/q quit",
        Mode::Image if image_count > 1 => "←/h prev  →/l next  Esc/q quit",
        Mode::Image => "Esc/q quit",
    }
}

/// Theme configuration loaded from file or using defaults.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Theme {
    /// Horizontal padding for highlight bar (inset from left/right)
    pub highlight_bar_hpad: Option<f32>,
    /// Vertical space before/after search box
    pub search_box_spacing: Option<f32>,
    /// Background color as hex string (e.g., "#5e3169")
    pub background_color: Option<String>,
    /// Foreground/text color as hex string (e.g., "#e8e8e6")
    pub text_color: Option<String>,
    /// Highlight color as hex string (e.g., "rgba(255, 153, 0, 1)")
    pub highlight_color: Option<String>,
    /// Default widget width in pixels
    pub default_width: Option<u32>,
    /// Width for the search box
    pub search_box_width: Option<u32>,
    /// Default widget height in pixels
    pub default_height: Option<u32>,
    /// Font size
    pub font_size: Option<f32>,
    /// Highlight font scale multiplier
    pub font_scale_highlight: Option<f32>,
    /// Use low contrast variant
    pub low_contrast: Option<bool>,
    /// Path to background image file
    pub background_image: Option<String>,
    /// Font scale multiplier for select highlight
    pub font_scale_select_highlight: Option<f32>,
    /// Native window corner radius in points. macOS only; ignored elsewhere. Requires making
    /// the window transparent, which happens automatically when this is set above 0.
    pub corner_radius: Option<f32>,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            background_color: None,
            text_color: None,
            highlight_color: Some("#ffffff".to_string()),
            default_width: Some(600),
            search_box_width: Some(360),
            default_height: Some(500),
            font_size: Some(40.0),
            font_scale_highlight: Some(1.6),
            low_contrast: Some(false),
            background_image: None,
            font_scale_select_highlight: Some(1.3),
            search_box_spacing: Some(40.0),
            highlight_bar_hpad: Some(8.0),
            // Matches macOS's own default window corner radius; no effect on other platforms
            // (see the `corner_radius` computation in `run`).
            corner_radius: Some(10.0),
        }
    }
}

impl Theme {
    /// Load a Theme from a TOML file at the given path.
    pub fn from_file(path: &str) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|toml| toml::from_str(&toml).ok())
            .unwrap_or_else(Theme::default)
    }

    /// Merges this theme with command-line argument overrides.
    /// CLI arguments take precedence over theme values.
    pub fn merge_with_args(&self, args: &Args) -> Theme {
        // For font_scale_highlight, use CLI if it differs from default (1.6), otherwise use theme
        let font_scale_highlight = if args.font_scale_highlight != 1.6 {
            Some(args.font_scale_highlight)
        } else {
            self.font_scale_highlight.or(Some(1.6))
        };

        // For font_scale_select_highlight, use CLI if provided, else theme, else default
        let font_scale_select_highlight = args.font_scale_select_highlight.or(self.font_scale_select_highlight).or(Some(1.3));
        let search_box_spacing = self.search_box_spacing.or(Some(40.0));
        let highlight_bar_hpad = self.highlight_bar_hpad.or(Some(8.0));

        Theme {
            background_color: args
                .background_color
                .clone()
                .or_else(|| self.background_color.clone()),
            text_color: args.text_color.clone().or_else(|| self.text_color.clone()),
            highlight_color: args
                .highlight_color
                .clone()
                .or_else(|| self.highlight_color.clone()),
            default_width: args.width.or(self.default_width),
            default_height: args.height.or(self.default_height),
            font_size: args.font_size.or(self.font_size).or(Some(40.0)),
            font_scale_highlight,
            low_contrast: if args.low_contrast {
                Some(true)
            } else {
                self.low_contrast
            },
            background_image: args
                .background_image
                .clone()
                .or_else(|| self.background_image.clone()),
            font_scale_select_highlight,
            search_box_spacing,
            highlight_bar_hpad,
            search_box_width: self.search_box_width.or(Some(120)),
            corner_radius: args.corner_radius.or(self.corner_radius),
        }
    }
}

/// Command-line arguments for configuring the vju application.
#[derive(Parser, Debug, PartialEq)]
#[command(version, about = "vju – visualization utility (recovered)")]
pub struct Args {
    #[arg(
        long,
        value_name = "KEYS",
        help = "When not in an input, exit and emit vju-key-[key] only for the given comma-separated keys (e.g. --return-keys r,t,escape)"
    )]
    pub return_keys: Option<String>,
    #[arg(short='v', long, action=clap::ArgAction::Count)]
    pub verbose: u8,
    #[arg(long = "type")]
    pub r#type: Option<String>,
    #[arg(long, help = "Show image from stdin")]
    pub image: bool,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long, help = "Show heading/title inside window")]
    pub show_title: bool,
    #[arg(long)]
    pub width: Option<u32>,
    #[arg(long)]
    pub height: Option<u32>,
    #[arg(long)]
    pub font_size: Option<f32>,
    #[arg(long, help = "Font scale multiplier for select highlight")]
    pub font_scale_select_highlight: Option<f32>,
    #[arg(long, help = "Native window corner radius in points (macOS only; ignored elsewhere)")]
    pub corner_radius: Option<f32>,
    #[arg(long)]
    pub background_color: Option<String>,
    #[arg(long)]
    pub text_color: Option<String>,
    #[arg(long, help = "Path to background image file")]
    pub background_image: Option<String>,
    #[arg(
        long,
        help = "Use lower contrast default theme (overrides default bg/fg unless explicit colors provided)"
    )]
    pub low_contrast: bool,
    #[arg(long, help = "Show key usage hint footer")]
    pub hint: bool,
    #[arg(long, help = "Show frame counter / debug info")]
    pub debug: bool,
    #[arg(long, help = "Hex highlight color (e.g. #ff9900)")]
    pub highlight_color: Option<String>,
    #[arg(long, help = "Highlight font scale multiplier", default_value_t = 1.6)]
    pub font_scale_highlight: f32,
    #[arg(long, help = "Center text horizontally (no vertical centering)")]
    pub center_text: bool,
    #[arg(long, help = "Use monospace font family")]
    pub monospace: bool,
    #[arg(long, help = "Additional font scale multiplier", default_value_t = 1.0)]
    pub font_scale: f32,
    #[arg(
        long = "return-pos",
        help = "In select mode: output zero-based index instead of item text on Enter"
    )]
    pub return_pos: bool,
    #[arg(
        long = "search-text-size",
        help = "Scale factor for search input text size",
        default_value_t = 1.0
    )]
    pub search_text_size: f32,
    #[arg(long, help = "Preselect item containing this string in select mode")]
    pub selected: Option<String>,
    #[arg(long, help = "Start in fullscreen mode")]
    pub fullscreen: bool,
    #[arg(
        long,
        help = "Render as the desktop background via wlr-layer-shell instead of a normal window (Wayland/wlroots compositors only, e.g. sway, Hyprland, river). Implies --image; takes one image (positional path or stdin). The process keeps running to keep the background surface alive."
    )]
    pub compositor_background: bool,
    #[arg(long, help = "Path to theme file (TOML format)")]
    pub theme: Option<String>,
    #[arg(long, help = "Show and focus the search input by default")]
    pub show_search: bool,
    #[arg(
        value_name = "IMAGES",
        help = "Image file paths to view (with --image). Shell-expanded globs work naturally, \
                e.g. `vju --image *.png`. If none given, reads a single image from stdin."
    )]
    pub images: Vec<String>,
}

/// Application mode determining behavior and interaction capabilities.
#[derive(Debug, PartialEq, Clone)]
pub enum Mode {
    /// View mode: read-only display of items with no selection capability.
    View,
    /// Select mode: interactive selection with keyboard navigation and fuzzy search.
    Select,
    /// Image mode: display an image from stdin
    Image,
}

/// Spacing configuration for UI layout elements.
#[derive(Clone, Copy)]
struct Spacing {
    heading_gap: f32,   // space after heading
    row_padding_y: f32, // per-row vertical padding (top + bottom)
    footer_gap_before_separator: f32, // space before footer separator
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            heading_gap: 6.0,
            row_padding_y: 4.0,
            footer_gap_before_separator: 4.0,
        }
    }
}

/// Main application state and UI controller.
///
/// Manages the display of items, user interaction, search filtering,
/// and rendering configuration for the vju visualization utility.
struct App {
    theme: Theme,
    highlight_text_color: egui::Color32,
    font_scale_select_highlight: f32,
    return_keys: Option<Vec<String>>,
    frames: u64,
    mode: Mode,
    items: Vec<String>,
    selected: usize,
    show_hint: bool,
    show_debug: bool,
    show_title: bool,
    title: String,
    first_logged: bool,
    highlight_color: egui::Color32,
    spacing: Spacing,
    center_text: bool,
    quivira_present: bool,
    fallback_logged: bool,
    widget_width: Option<f32>,
    widget_height: Option<f32>,
    return_pos: bool,
    // --- Added for fuzzy search ---
    search_input: String,
    filtered_items: Vec<usize>,
    show_search: bool,
    search_just_activated: bool,
    omit_first_slash: bool,
    fullscreen: bool,
    request_fullscreen: bool,
    background_texture: Option<egui::TextureHandle>,
    // For image mode: all images already uploaded once at startup, not re-uploaded every
    // frame. Empty if decoding failed or produced nothing.
    images: Vec<egui::TextureHandle>,
    current_image: usize,
}

impl App {
    /// Creates a new App instance with the specified configuration.
    ///
    /// # Arguments
    ///
    /// * `mode` - The application mode (View or Select)
    /// * `items` - Vector of strings to display
    /// * `show_hint` - Whether to show keyboard hint footer
    /// * `show_debug` - Whether to show frame counter debug info
    /// * `show_title` - Whether to show heading/title inside window
    /// * `title` - The title text drawn as a heading when `show_title` is set
    /// * `highlight_color` - Color for selected item highlight
    /// * `highlight_scale` - Scale multiplier for selected item text
    /// * `spacing` - Spacing configuration for UI elements
    /// * `center_text` - Whether to center text horizontally
    /// * `quivira_present` - Whether Quivira font fallback is available
    /// * `widget_width` - Optional widget width constraint
    /// * `widget_height` - Optional widget height constraint
    /// * `return_pos` - In select mode: output index instead of text on Enter
    /// * `selected` - Optional string to preselect in select mode
    /// * `fullscreen` - Whether to start in fullscreen mode
    /// * `background_texture` - Optional background image texture
    /// * `images` - Image-mode textures, already uploaded once by the caller. May be empty.
    fn new(
        theme: &Theme,
        mode: Mode,
        items: Vec<String>,
        show_hint: bool,
        font_scale_select_highlight: f32,
        show_debug: bool,
        show_title: bool,
        title: String,
        highlight_color: egui::Color32,
        spacing: Spacing,
        center_text: bool,
        quivira_present: bool,
        widget_width: Option<f32>,
        widget_height: Option<f32>,
        return_pos: bool,
        selected: Option<String>,
        fullscreen: bool,
        background_texture: Option<egui::TextureHandle>,
        show_search_flag: bool,
        return_keys: Option<String>,
        images: Vec<egui::TextureHandle>,
    ) -> Self {
        let filtered_items: Vec<usize> = (0..items.len()).collect();
        let mut selected_idx = 0;
        if let (Mode::Select, Some(sel)) = (&mode, selected.as_ref()) {
            for (i, item) in items.iter().enumerate() {
                if item.contains(sel) {
                    selected_idx = i;
                    break;
                }
            }
        }
        // Highlighted text should use the theme's text color for visibility
        let highlight_text_color = theme.text_color
            .as_ref()
            .and_then(|s| parse_hex_color(s))
            .unwrap_or(egui::Color32::BLACK);

        Self {
            theme: theme.clone(),
            frames: 0,
            mode,
            items,
            selected: selected_idx,
            show_hint,
            show_debug,
            show_title,
            title,
            first_logged: false,
            highlight_color,
            spacing,
            center_text,
            font_scale_select_highlight,
            quivira_present,
            fallback_logged: false,
            widget_width,
            widget_height,
            return_pos,
            search_input: String::new(),
            filtered_items,
            show_search: show_search_flag,
            search_just_activated: show_search_flag,
            omit_first_slash: false,
            fullscreen,
            background_texture,
            return_keys: return_keys
                .map(|s| s.split(',').map(|k| k.trim().to_lowercase()).collect()),
            images,
            current_image: 0,
            request_fullscreen: false,
            highlight_text_color,
        }
    }

    /// Processes keyboard input events and updates application state accordingly.
    ///
    /// Handles navigation (arrow keys, j/k, G), search activation (/),
    /// selection (Enter), and quit (Esc/q).
    ///
    /// # Arguments
    ///
    /// * `ctx` - The egui context for accessing input events
    fn process_input(&mut self, ctx: &egui::Context) {
        ctx.input(|inp| {
            for ev in &inp.events {
                // Always handle navigation and selection keys in select mode
                // Always quit on ESC
                if let egui::Event::Key { key: egui::Key::Escape, pressed: true, .. } = ev {
                    print_and_exit("vju-exit");
                }
                // Always quit on 'q' or 'Q' (except when search is focused)
                if !self.show_search {
                    match ev {
                        egui::Event::Key { key: egui::Key::Q, pressed: true, .. } => {
                            print_and_exit("vju-exit");
                        }
                        egui::Event::Text(t) => {
                            if matches!(t.as_str(), "q" | "Q") {
                                print_and_exit("vju-exit");
                            }
                        }
                        _ => {}
                    }
                }
                match ev {
                    egui::Event::Key { key: egui::Key::ArrowUp, pressed: true, .. } if matches!(self.mode, Mode::Select) => {
                        if self.filtered_items.is_empty() { return; }
                        if self.selected > 0 {
                            self.selected -= 1;
                        } else {
                            self.selected = self.filtered_items.len() - 1;
                        }
                    }
                    egui::Event::Key { key: egui::Key::ArrowDown, pressed: true, .. } if matches!(self.mode, Mode::Select) => {
                        if self.filtered_items.is_empty() { return; }
                        if self.selected + 1 < self.filtered_items.len() {
                            self.selected += 1;
                        } else {
                            self.selected = 0;
                        }
                    }
                    egui::Event::Key { key: egui::Key::Tab, pressed: true, .. } if matches!(self.mode, Mode::Select) => {
                        if self.filtered_items.is_empty() { return; }
                        self.selected = (self.selected + 1) % self.filtered_items.len();
                    }
                    egui::Event::Text(t) if matches!(self.mode, Mode::Select) && matches!(t.as_str(), "j" | "J") => {
                        if self.filtered_items.is_empty() { return; }
                        if self.selected + 1 < self.filtered_items.len() {
                            self.selected += 1;
                        } else {
                            self.selected = 0;
                        }
                    }
                    egui::Event::Text(t) if matches!(self.mode, Mode::Select) && matches!(t.as_str(), "k" | "K") => {
                        if self.filtered_items.is_empty() { return; }
                        if self.selected > 0 {
                            self.selected -= 1;
                        } else {
                            self.selected = self.filtered_items.len() - 1;
                        }
                    }
                    egui::Event::Text(t) if matches!(self.mode, Mode::Select) && t == "G" => {
                        self.selected = self.filtered_items.len().saturating_sub(1);
                    }
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } if matches!(self.mode, Mode::Select) => {
                        if let Some(&idx) = self.filtered_items.get(self.selected) {
                            if self.return_pos {
                                print_flushed(&idx.to_string());
                            } else if let Some(s) = self.items.get(idx) {
                                print_flushed(s);
                            }
                            std::process::exit(0);
                        }
                    }
                    // Image navigation (only meaningful with more than one image, but harmless
                    // no-ops otherwise since `next_image`/`prev_image` handle len() <= 1).
                    egui::Event::Key { key: egui::Key::ArrowRight, pressed: true, .. } if matches!(self.mode, Mode::Image) => {
                        self.next_image();
                    }
                    egui::Event::Text(t) if matches!(self.mode, Mode::Image) && matches!(t.as_str(), "l" | "L") => {
                        self.next_image();
                    }
                    egui::Event::Key { key: egui::Key::ArrowLeft, pressed: true, .. } if matches!(self.mode, Mode::Image) => {
                        self.prev_image();
                    }
                    egui::Event::Text(t) if matches!(self.mode, Mode::Image) && matches!(t.as_str(), "h" | "H") => {
                        self.prev_image();
                    }
                    // Fuzzy search activation
                    egui::Event::Text(t) if matches!(self.mode, Mode::Select) => {
                        if t == "/" && !self.show_search {
                            self.show_search = true;
                            self.search_just_activated = true;
                            self.omit_first_slash = true;
                        }
                    }
                    // --return-keys: exit and emit vju-key-[key] only for specified keys (not in input/search)
                    _ => {
                        if let Some(ref keys) = self.return_keys {
                            if !self.show_search {
                                let mut emit_key = None;
                                match ev {
                                    // 'q' and 'Q' quit logic moved to top-level event loop. These two
                                    // must come before the generic Key/Text arms below, which would
                                    // otherwise match first and make these unreachable.
                                    //
                                    // TODO: undecided whether this should fire whenever --return-keys is
                                    // set at all (current behavior) or only when "r" is explicitly listed
                                    // in --return-keys (in which case it's redundant with vju-key-r and
                                    // could be deleted). Not documented in README either way.
                                    egui::Event::Key { key: egui::Key::R, pressed: true, .. } => {
                                        print_and_exit("vju-read");
                                    }
                                    egui::Event::Text(t) if matches!(t.as_str(), "r" | "R") => {
                                        print_and_exit("vju-read");
                                    }
                                    egui::Event::Key { key, pressed: true, .. } => {
                                        let key_name = format!("{:?}", key).to_lowercase();
                                        if keys.contains(&key_name) {
                                            emit_key = Some(key_name);
                                        }
                                    }
                                    egui::Event::Text(t) => {
                                        let t_lc = t.to_lowercase();
                                        if keys.contains(&t_lc) {
                                            emit_key = Some(t_lc);
                                        }
                                    }
                                    _ => {}
                                }
                                if let Some(key) = emit_key {
                                    let selection = if matches!(self.mode, Mode::Select) {
                                        if let Some(&idx) = self.filtered_items.get(self.selected) {
                                            if self.return_pos {
                                                format!(":{}", idx)
                                            } else if let Some(s) = self.items.get(idx) {
                                                format!(":{}", s)
                                            } else {
                                                String::new()
                                            }
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    };
                                    print_and_exit(&format!("vju-key-{}{}", key, selection));
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /// Updates the filtered items list based on the current search input.
    ///
    /// Performs case-insensitive substring matching. If search is empty,
    /// shows all items. Resets selection if it becomes out of bounds.
    fn update_filtered_items(&mut self) {
        self.filtered_items = filter_indices(&self.items, &self.search_input);
        self.selected = clamp_selected(self.selected, self.filtered_items.len());
    }

    /// Advances to the next image (image mode), wrapping around at the end. A no-op with 0 or
    /// 1 images.
    fn next_image(&mut self) {
        if self.images.len() > 1 {
            self.current_image = (self.current_image + 1) % self.images.len();
        }
    }

    /// Moves to the previous image (image mode), wrapping around at the start. A no-op with 0
    /// or 1 images.
    fn prev_image(&mut self) {
        if self.images.len() > 1 {
            self.current_image = (self.current_image + self.images.len() - 1) % self.images.len();
        }
    }

    /// Draws the highlight bar for a selected row, spanning the full width with horizontal padding.
    fn draw_highlight_bar(
        &self,
        ui: &egui::Ui,
        top: f32,
        bottom: f32,
        hpad: f32,
        color: egui::Color32,
    ) {
        let left = ui.max_rect().left() + hpad;
        let right = ui.max_rect().right() - hpad;
        let rect = egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom));
        ui.painter().rect_filled(rect, 4.0, color);
    }

    /// Renders `self.filtered_items`, scrolling automatically so the selected row stays in
    /// view. Shared by select and view mode: `highlight` controls whether the selected row
    /// gets a highlight bar and distinct text color (select mode only; view mode has no
    /// selection to highlight, and just lists everything).
    fn draw_item_list(&self, ui: &mut egui::Ui, font_scale: f32, left_pad: f32, highlight: bool) {
        let row_height = ui.text_style_height(&egui::TextStyle::Body) * font_scale + self.spacing.row_padding_y;
        let max_rows = (ui.available_height() / row_height).floor() as usize;
        let hpad = self.theme.highlight_bar_hpad.unwrap_or(8.0);
        let start = if self.selected >= max_rows { self.selected - max_rows + 1 } else { 0 };
        for (i, &idx) in self.filtered_items.iter().enumerate().skip(start).take(max_rows) {
            let is_selected = highlight && i == self.selected;
            let (_resp, rect) = ui.allocate_space(egui::vec2(ui.available_width(), row_height));
            if is_selected {
                self.draw_highlight_bar(ui, rect.min.y, rect.max.y, hpad, self.highlight_color);
            }
            let text_color = if is_selected { self.highlight_text_color } else { ui.visuals().text_color() };
            let font_id = egui::FontId {
                size: ui.text_style_height(&egui::TextStyle::Body) * font_scale,
                family: egui::FontFamily::Proportional,
            };
            let (pos, align) = if self.center_text {
                (rect.center(), egui::Align2::CENTER_CENTER)
            } else {
                (egui::pos2(rect.min.x + left_pad, rect.center().y), egui::Align2::LEFT_CENTER)
            };
            ui.painter().text(pos, align, &self.items[idx], font_id, text_color);
        }
    }

    /// Keyboard hint text for the current mode, shown in the `--hint` footer.
    fn hint_text(&self) -> &'static str {
        hint_text_for_mode(&self.mode, self.images.len())
    }

    /// Draws the optional hint footer (`--hint`) and frame counter (`--debug`) below whatever
    /// content was just rendered.
    fn draw_footer(&self, ui: &mut egui::Ui) {
        if self.show_hint {
            ui.add_space(self.spacing.footer_gap_before_separator);
            ui.separator();
            ui.weak(self.hint_text());
        }
        if self.show_debug {
            ui.weak(format!("Frame {}", self.frames));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.process_input(ui.ctx());
        // Apply fullscreen toggle after UI code to avoid deadlock
        if self.request_fullscreen {
            self.fullscreen = !self.fullscreen;
            ui.ctx().send_viewport_cmd(egui::viewport::ViewportCommand::Fullscreen(self.fullscreen));
            self.request_fullscreen = false;
        }
        let bg = ui.style().visuals.panel_fill;
        let panel_bg = if self.background_texture.is_some() {
            egui::Color32::TRANSPARENT
        } else {
            bg
        };

        // Same value that used to only push content down from the top, now applied as a
        // uniform margin on all four sides of the window -- except in image mode, which stays
        // edge-to-edge.
        let window_margin = if matches!(self.mode, Mode::Image) {
            0.0
        } else {
            self.theme.search_box_spacing.unwrap_or(40.0)
        };
        egui::CentralPanel::default().frame(egui::Frame::default().fill(panel_bg).inner_margin(window_margin)).show(ui, |ui| {
            self.frames += 1;
            // Draw background image if available (behind all content)
            if let Some(ref texture) = self.background_texture {
                let rect = ui.max_rect();
                ui.painter().image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            if self.show_title {
                ui.heading(&self.title);
                ui.add_space(self.spacing.heading_gap);
            }
            if matches!(self.mode, Mode::Image) {
                if let Some(texture) = self.images.get(self.current_image) {
                    ui.image(texture);
                } else if self.images.is_empty() {
                    ui.colored_label(egui::Color32::RED, "Failed to decode image from stdin");
                }
                if self.images.len() > 1 {
                    // Small "N/total" indicator in the corner; doesn't affect layout since it's
                    // painted directly rather than allocated as a widget.
                    let rect = ui.max_rect();
                    let font_id = egui::TextStyle::Small.resolve(ui.style());
                    ui.painter().text(
                        rect.right_bottom() - egui::vec2(8.0, 4.0),
                        egui::Align2::RIGHT_BOTTOM,
                        format!("{}/{}", self.current_image + 1, self.images.len()),
                        font_id,
                        ui.visuals().text_color(),
                    );
                }
                if self.quivira_present && !self.fallback_logged {
                    if self
                        .items
                        .iter()
                        .any(|line| line.chars().any(|c| c as u32 > 0x7F))
                    {
                        log::info!("Quivira fallback likely used (detected non-ASCII glyphs in input)");
                        self.fallback_logged = true;
                    }
                }
                if !self.first_logged {
                    let rect = ui.ctx().input(|i| i.content_rect());
                    let widget_w = self.widget_width.unwrap_or(rect.width());
                    let widget_h = self.widget_height.unwrap_or(rect.height());
                    log::info!("frame0 viewport pos=({}, {}) size=({:.0}x{:.0}) widget=({:.0}x{:.0}) items={} selected={} mode={:?}", rect.min.x, rect.min.y, rect.width(), rect.height(), widget_w, widget_h, self.items.len(), self.selected, self.mode);
                    self.first_logged = true;
                }
                self.draw_footer(ui);
                return;
            }

            // --- Search box ---
            if matches!(self.mode, Mode::Select) || self.show_search {
                if self.show_search {
                    let search_id = egui::Id::new("search_box");
                    if self.search_just_activated {
                        ui.ctx().memory_mut(|mem| mem.request_focus(search_id));
                        self.search_just_activated = false;
                    }
                    use egui::{Frame, CornerRadius, Stroke, Color32};
                    let search_box_bg = Color32::from_rgba_unmultiplied(240, 232, 255, 120); // light purple, more transparent
                    let search_box_stroke = Stroke::new(1.5, Color32::from_rgb(120, 80, 180));
                    let search_box_corner_radius = CornerRadius::same(10);
                    let search_box_margin = egui::Vec2::splat(8.0);
                    ui.vertical_centered(|ui| {
                        let input_width = self.theme.search_box_width.unwrap_or(300) as f32;
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(input_width, 0.0),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                Frame::new()
                                    .fill(search_box_bg)
                                    .stroke(search_box_stroke)
                                    .corner_radius(search_box_corner_radius)
                                    .inner_margin(search_box_margin)
                                    .show(ui, |ui| {
                                        let mut te = egui::TextEdit::singleline(&mut self.search_input)
                                            .desired_width(input_width)
                                            .frame(Frame::NONE)
                                            .id(egui::Id::new("search_box"))
                                            .horizontal_align(egui::Align::Center);
                                        // Scale font size for search input (fixed 1.4)
                                        let font_id = egui::TextStyle::Body.resolve(ui.style());
                                        let scaled_font = egui::FontId {
                                            size: font_id.size * 1.4,
                                            family: font_id.family.clone(),
                                        };
                                        te = te.font(scaled_font);
                                        let resp = ui.add(te);
                                        if self.omit_first_slash && self.search_input == "/" {
                                            self.search_input.clear();
                                            self.omit_first_slash = false;
                                        }
                                        if resp.changed() {
                                            self.update_filtered_items();
                                        }
                                    });
                            },
                        );
                    });
                    ui.add_space(window_margin);
                }
            }

            // --- List rendering for select/view mode ---
            let hpad = self.theme.highlight_bar_hpad.unwrap_or(8.0);
            if matches!(self.mode, Mode::Select) {
                self.draw_item_list(ui, self.font_scale_select_highlight, hpad, true);
            }
            if matches!(self.mode, Mode::View) {
                self.draw_item_list(ui, 1.0, hpad, false);
            }
            if matches!(self.mode, Mode::Select | Mode::View) {
                self.draw_footer(ui);
            }
        });
    }
}

/// Loads an image from a file and converts it to an egui texture.
///
/// # Arguments
///
/// * `ctx` - The egui context for creating textures
/// * `path` - Path to the image file
///
/// # Returns
///
/// Returns `Some(TextureHandle)` if the image was successfully loaded, `None` otherwise.
fn load_background_image(ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
    match image::open(path) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let width = rgba.width() as usize;
            let height = rgba.height() as usize;
            let size = [width, height];
            let pixels: Vec<egui::Color32> = rgba
                .pixels()
                .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                .collect();
            let color_image = egui::ColorImage {
                size,
                pixels,
                source_size: egui::Vec2::new(width as f32, height as f32),
            };
            let texture = ctx.load_texture(
                "background_image",
                color_image,
                egui::TextureOptions::LINEAR,
            );
            log::info!(
                "Loaded background image from {} ({}x{})",
                path,
                width,
                height
            );
            Some(texture)
        }
        Err(e) => {
            log::warn!("Failed to load background image from {}: {}", path, e);
            None
        }
    }
}

/// Configures fonts and styling for the egui context.
///
/// Sets up font sizes, loads Quivira font fallback from assets/ if available,
/// searches for system Noto fonts, and optionally loads system monospace fonts.
///
/// # Arguments
///
/// * `ctx` - The egui context to configure
/// * `fg` - Foreground text color
/// * `bg` - Background panel color
/// * `font_size` - Base font size for Body text style
/// * `font_scale_extra` - Additional scale multiplier for all fonts
/// * `use_monospace` - Whether to use monospace font family
///
/// # Returns
///
/// Returns `true` if Quivira font was successfully loaded, `false` otherwise.
fn setup_fonts(
    ctx: &egui::Context,
    fg: egui::Color32,
    bg: egui::Color32,
    font_size: f32,
    font_scale_extra: f32,
    use_monospace: bool,
) -> bool {
    // Applied to both light and dark styles (all_styles_mut) so this custom theme doesn't get
    // undone if the OS theme preference changes at runtime.
    ctx.all_styles_mut(|s| s.visuals.override_text_color = Some(fg));
    // Base scale: keep pixels_per_point close to 1 and adjust text style sizes directly for clearer font-size semantics.
    ctx.set_pixels_per_point(1.0);
    // Derive actual sizes: treat provided font_size as Body size; Heading 1.6x, Button 1.0x, Small 0.85x, Monospace same as Body.
    let body_sz = font_size * font_scale_extra;
    let heading_sz = body_sz * 1.6;
    let button_sz = body_sz;
    let small_sz = body_sz * 0.85;
    ctx.all_styles_mut(|style| {
        style.visuals.widgets.noninteractive.bg_fill = bg;
        style.visuals.panel_fill = bg;
        for (ts, sz) in [
            (egui::TextStyle::Body, body_sz),
            (egui::TextStyle::Button, button_sz),
            (egui::TextStyle::Heading, heading_sz),
            (egui::TextStyle::Small, small_sz),
            (egui::TextStyle::Monospace, body_sz),
        ] {
            if let Some(font) = style.text_styles.get_mut(&ts) {
                font.size = sz;
            }
        }
    });
    log::info!(
        "font_config body={:.1} heading={:.1} button={:.1} small={:.1} scale_extra={:.2}",
        body_sz,
        heading_sz,
        button_sz,
        small_sz,
        font_scale_extra
    );
    let mut fonts = egui::FontDefinitions::default();
    let mut quivira_present = false;
    if let Ok(bytes) =
        std::fs::read("assets/Quivira.otf").or_else(|_| std::fs::read("assets/Quivira.ttf"))
    {
        let len = bytes.len();
        fonts.font_data.insert(
            "quivira".into(),
            egui::FontData {
                font: std::borrow::Cow::Owned(bytes),
                index: 0,
                tweak: egui::FontTweak::default(),
            }
            .into(),
        );
        for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(fam)
                .or_default()
                .push("quivira".into());
        }
        log::info!("Added Quivira font fallback ({} bytes)", len);
        quivira_present = true;
    } else {
        log::debug!("Quivira font not found; skipping fallback");
    }

    // System search for Noto fonts (macOS/Linux typical locations). Prepend if found.
    let noto_system_candidates = [
        (
            "noto_sans",
            "/System/Library/Fonts/NotoSans-Regular.ttc",
            egui::FontFamily::Proportional,
        ),
        (
            "noto_sans",
            "/Library/Fonts/NotoSans-Regular.ttf",
            egui::FontFamily::Proportional,
        ),
        (
            "noto_sans",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            egui::FontFamily::Proportional,
        ),
        (
            "noto_sans",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            egui::FontFamily::Proportional,
        ),
        (
            "noto_sans_mono",
            "/System/Library/Fonts/NotoSansMono-Regular.ttc",
            egui::FontFamily::Monospace,
        ),
        (
            "noto_sans_mono",
            "/Library/Fonts/NotoSansMono-Regular.ttf",
            egui::FontFamily::Monospace,
        ),
        (
            "noto_sans_mono",
            "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
            egui::FontFamily::Monospace,
        ),
        (
            "noto_sans_mono",
            "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
            egui::FontFamily::Monospace,
        ),
    ];
    for (key, path, fam) in noto_system_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                key.into(),
                egui::FontData {
                    font: std::borrow::Cow::Owned(bytes),
                    index: 0,
                    tweak: egui::FontTweak::default(),
                }
                .into(),
            );
            let list = fonts.families.entry(fam).or_default();
            if !list.iter().any(|n| n == key) {
                list.insert(0, key.into());
            }
            log::info!("Loaded system Noto font '{}'", path);
        } else {
            log::debug!("System Noto font not found at {}", path);
        }
    }
    // Automatic monospace font attempt (macOS common fonts) when flag set
    if use_monospace {
        let candidates = [
            "/System/Library/Fonts/SFNSMono.ttf",
            "/System/Library/Fonts/SFMono-Regular.ttf",
            "/System/Library/Fonts/Menlo.ttc",
            "/System/Library/Fonts/Menlo-Regular.ttf",
        ];
        for cand in candidates.iter() {
            if let Ok(bytes) = std::fs::read(cand) {
                fonts.font_data.insert(
                    "system_mono".into(),
                    egui::FontData {
                        font: std::borrow::Cow::Owned(bytes),
                        index: 0,
                        tweak: egui::FontTweak::default(),
                    }
                    .into(),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .insert(0, "system_mono".into());
                log::info!("Loaded system monospace font: {}", cand);
                break;
            }
        }
    }
    ctx.set_fonts(fonts.clone());
    // Log final ordered font families for verification
    for (fam, list) in &fonts.families {
        let fname = match fam {
            egui::FontFamily::Proportional => "Proportional",
            egui::FontFamily::Monospace => "Monospace",
            _ => "Custom",
        };
        log::debug!("Font family {} order: {}", fname, list.join(", "));
    }
    if use_monospace {
        use egui::FontFamily::Monospace;
        ctx.all_styles_mut(|s| {
            for ts in [
                egui::TextStyle::Body,
                egui::TextStyle::Small,
                egui::TextStyle::Button,
                egui::TextStyle::Heading,
                egui::TextStyle::Monospace,
            ] {
                if let Some(font) = s.text_styles.get_mut(&ts) {
                    font.family = Monospace;
                }
            }
        });
    }
    quivira_present
}

/// Rounds the native window's corners (macOS only -- a no-op on every other platform, so
/// callers don't need to `#[cfg]` the call site). `radius` is in points; the window must
/// already be transparent (`ViewportBuilder::with_transparent(true)`) or the area outside the
/// rounded corners will show as opaque instead of see-through.
///
/// Sets the corner radius on the `NSView`'s existing `CALayer` rather than forcing a fresh one
/// via `setWantsLayer` unconditionally, since the view is very likely already layer-backed by
/// wgpu's Metal backend by the time this runs (`eframe`'s window/surface is created before the
/// app-creation callback we call this from) -- we don't want to disturb that.
#[cfg(target_os = "macos")]
fn apply_native_corner_radius(window_handle: raw_window_handle::WindowHandle<'_>, radius: f32) {
    let raw_window_handle::RawWindowHandle::AppKit(handle) = window_handle.as_raw() else {
        return;
    };
    unsafe {
        let view_ptr: *mut objc2_app_kit::NSView = handle.ns_view.as_ptr().cast();
        let view: &objc2_app_kit::NSView = &*view_ptr;

        if !view.wantsLayer() {
            view.setWantsLayer(true);
        }
        if let Some(layer) = view.layer() {
            layer.setCornerRadius(radius as f64);
            layer.setMasksToBounds(true);
        }
        if let Some(window) = view.window() {
            window.setOpaque(false);
            window.setBackgroundColor(Some(&objc2_app_kit::NSColor::clearColor()));
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_native_corner_radius(_window_handle: raw_window_handle::WindowHandle<'_>, _radius: f32) {}

/// Runs the vju application: parses command-line arguments, reads input from stdin,
/// configures the GUI, and launches the egui application window.
pub fn run() -> Result<(), eframe::Error> {
    let args = Args::parse();
    let title = args.title.clone().unwrap_or_else(|| "vju".into());
    if std::env::var("RUST_LOG").is_err() {
        use log::LevelFilter;
        let mut b = env_logger::Builder::from_default_env();
        b.filter_level(match args.verbose {
            0 => LevelFilter::Info,
            1 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        })
        .init();
    } else {
        env_logger::init();
    }
    let (mode, stdin_items, image_inputs): (Mode, Vec<String>, Vec<Vec<u8>>) = if args.image || args.compositor_background {
        // Positional file paths (shell-expanded globs work naturally here, e.g.
        // `vju --image *.png`) take precedence; falls back to a single image on stdin.
        let images = if !args.images.is_empty() {
            args.images
                .iter()
                .filter_map(|path| match std::fs::read(path) {
                    Ok(bytes) => Some(bytes),
                    Err(e) => {
                        log::warn!("Failed to read image file '{}': {}", path, e);
                        None
                    }
                })
                .collect()
        } else {
            use std::io::Read;
            let mut buf = Vec::new();
            if std::io::stdin().read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                vec![buf]
            } else {
                Vec::new()
            }
        };
        (Mode::Image, Vec::new(), images)
    } else if matches!(args.r#type.as_deref(), Some("select")) {
        use std::io::{self, Read};
        let mut buf = String::new();
        if io::stdin().read_to_string(&mut buf).is_ok() {
            let items = buf
                .lines()
                .map(|l| l.trim_end_matches(['\r', '\n']))
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            (Mode::Select, items, Vec::new())
        } else {
            (Mode::Select, Vec::new(), Vec::new())
        }
    } else {
        use std::io::{self, Read};
        let mut buf = String::new();
        if io::stdin().read_to_string(&mut buf).is_ok() {
            let items = buf
                .lines()
                .map(|l| l.trim_end_matches(['\r', '\n']))
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            (Mode::View, items, Vec::new())
        } else {
            (Mode::View, Vec::new(), Vec::new())
        }
    };
    if matches!(mode, Mode::Image) {
        log::info!("Loaded {} image(s) (mode={:?})", image_inputs.len(), mode);
    } else {
        log::info!("Loaded {} input items (mode={:?})", stdin_items.len(), mode);
    }

    if args.compositor_background {
        let Some(image_bytes) = image_inputs.first() else {
            eprintln!("vju: --compositor-background requires at least one image");
            std::process::exit(1);
        };
        if let Err(e) = compositor_background::run(image_bytes.clone()) {
            eprintln!("vju: failed to set compositor background: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    // Load theme from file or use default

    let theme = if let Some(theme_path) = &args.theme {
        Theme::from_file(theme_path)
    } else if let Some(config_path) = find_config_path() {
        Theme::from_file(&config_path)
    } else {
        Theme::default()
    };

    // Merge theme with CLI arguments (CLI takes precedence)
    let theme = theme.merge_with_args(&args);

    // Decode all images once (image mode), covering both raster formats and SVG, and reuse the
    // same decoded textures in App::new below instead of decoding bytes twice.
    let screen_width = 1920.0;
    let image_max_width = (screen_width * 0.8) as u32;
    let decoded_images: Vec<(egui::ColorImage, [usize; 2])> = if matches!(mode, Mode::Image) {
        image_inputs
            .iter()
            .filter_map(|bytes| decode_image(bytes, image_max_width, u32::MAX))
            .collect()
    } else {
        Vec::new()
    };

    // Determine window size: in image mode, auto-size to fit the largest decoded image
    // (edge-to-edge, no margin) so the window isn't stuck at the default 600x500 regardless of
    // what's being displayed. Smaller or differently-shaped images in the set are letterboxed
    // rather than shrinking the window down for them.
    let (widget_width, widget_height) = if matches!(mode, Mode::Image) {
        let max_w = decoded_images.iter().map(|(_, size)| size[0]).max();
        let max_h = decoded_images.iter().map(|(_, size)| size[1]).max();
        match (max_w, max_h) {
            (Some(w), Some(h)) => (Some(w as f32), Some(h as f32)),
            _ => (Some(800.0), Some(600.0)),
        }
    } else {
        (theme.default_width.map(|v| v as f32), theme.default_height.map(|v| v as f32))
    };

    // Default palette: original was a high-contrast white on purple. We now offer a gentler variant.
    let low_contrast_requested = theme.low_contrast.unwrap_or(false);
    //let default_bg = if low_contrast_requested { egui::Color32::from_rgb(0x4d,0x2a,0x57) } else { egui::Color32::from_rgb(0x5e,0x31,0x69) }; // slightly deeper/duller purple when low_contrast
    let default_bg = if low_contrast_requested {
        egui::Color32::from_rgb(60, 15, 60)
    } else {
        egui::Color32::from_rgb(40, 5, 40)
    }; // slightly deeper/duller purple when low_contrast
       // Off-white foreground to reduce glare vs pure white; slightly more muted when low_contrast.
       // Foreground: keep "white" appearance but soften (reduce blue bias and extreme brightness).
       // Standard: slightly off-white #F2F2F0; Low contrast: one step dimmer #ECEBE8.
    let default_fg = if low_contrast_requested {
        egui::Color32::from_rgb(0xE4, 0xE3, 0xE0)
    } else {
        egui::Color32::from_rgb(0xE8, 0xE8, 0xE6)
    };
    let bg = theme
        .background_color
        .as_ref()
        .and_then(|c| egui::Color32::from_hex(c).ok())
        .unwrap_or(default_bg);
    let fg = theme
        .text_color
        .as_ref()
        .and_then(|c| egui::Color32::from_hex(c).ok())
        .unwrap_or(default_fg);
    let font_size = theme.font_size.unwrap_or(24.0);
    let font_scale_extra = args.font_scale;
    let highlight_color = theme
        .highlight_color
        .as_ref()
        .and_then(|c| parse_hex_color(c))
        .unwrap_or(egui::Color32::BLACK);
    let center_text = args.center_text;
    let use_monospace = args.monospace;
    // Window size: use provided width/height if present, else defaults.
    let win_w = widget_width.unwrap_or(600.0);
    let win_h = widget_height.unwrap_or(500.0);
    // Corner radius defaults on (see Theme::default), but only ever takes effect on macOS
    // (see apply_native_corner_radius). Gating it here too, not just in that function, so
    // non-macOS builds never flip on window transparency below for a feature that's a no-op
    // for them anyway -- transparency needs a compositor to look right, and forcing it on by
    // default on e.g. compositor-less X11 setups would be a visual regression for no benefit.
    let corner_radius = if cfg!(target_os = "macos") {
        theme.corner_radius.filter(|r| *r > 0.0)
    } else {
        None
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([win_w, win_h])
        .with_decorations(false)
        .with_transparent(corner_radius.is_some());
    if args.fullscreen {
        viewport = viewport.with_fullscreen(true);
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    // Automatic gap: monospace gets no vertical gap; proportional keeps default spacing value.
    let auto_gap = 12.0; // increase vertical gap for even more visible spacing
    log::info!(
        "startup window_size=({}x{}) widget_target=({}x{}) monospace={} auto_gap={}",
        win_w as u32,
        win_h as u32,
        widget_width.unwrap_or(win_w) as u32,
        widget_height.unwrap_or(win_h) as u32,
        use_monospace,
        auto_gap
    );
    let show_hint = args.hint;
    let show_debug = args.debug;
    let show_title = args.show_title;
    let return_pos = args.return_pos;
    let background_image_path = theme.background_image.clone();
    let app_title = title.clone();
    eframe::run_native(
        &title,
        native_options,
        Box::new(move |cc| {
            if let Some(radius) = corner_radius {
                use raw_window_handle::HasWindowHandle;
                if let Ok(handle) = cc.window_handle() {
                    apply_native_corner_radius(handle, radius);
                }
            }
            let quivira_present = setup_fonts(
                &cc.egui_ctx,
                fg,
                bg,
                font_size,
                font_scale_extra,
                use_monospace,
            );
            let mut spacing = Spacing::default();
            spacing.row_padding_y = auto_gap;
            let background_texture = background_image_path
                .as_ref()
                .and_then(|path| load_background_image(&cc.egui_ctx, path));
            // Upload once here rather than in App::ui, which runs every frame.
            let images: Vec<egui::TextureHandle> = decoded_images
                .into_iter()
                .enumerate()
                .map(|(i, (color_image, _size))| {
                    cc.egui_ctx.load_texture(
                        &format!("image_{i}"),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    )
                })
                .collect();
            let show_search_flag = args.show_search;
            let mode_clone = mode.clone();
            Ok(Box::new(App::new(
                &theme,
                mode_clone,
                stdin_items,
                show_hint,
                theme.font_scale_select_highlight.unwrap_or(1.3),
                show_debug,
                show_title,
                app_title,
                highlight_color,
                spacing,
                center_text,
                quivira_present,
                widget_width,
                widget_height,
                return_pos,
                args.selected,
                args.fullscreen,
                background_texture,
                show_search_flag,
                args.return_keys,
                images,
            )))
        }),
    )
}
