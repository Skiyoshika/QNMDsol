use std::time::Instant;

use eframe::{egui, CreationContext};
use egui_plot::{Line, Plot, PlotPoints};
use rand::{rngs::StdRng, Rng, SeedableRng};
use waveform_pipeline::{TimeWindow, WaveformPipeline, YScale};

const SAMPLE_RATE_HZ: f32 = 250.0;
const CHANNELS: usize = 4;
const WINDOW_SECS: f32 = 5.0;

struct SignalGen {
    freq_hz: f32,
    phase: f32,
    amp_uv: f32,
    noise_uv: f32,
}

impl SignalGen {
    fn sample(&self, t: f32, rng: &mut StdRng) -> f32 {
        let base = (2.0 * std::f32::consts::PI * self.freq_hz * t + self.phase).sin()
            * self.amp_uv;
        let noise = rng.gen_range(-self.noise_uv..self.noise_uv);
        base + noise
    }
}

struct DemoApp {
    pipeline: WaveformPipeline,
    started_at: Instant,
    last_ts: f32,
    rng: StdRng,
    gens: Vec<SignalGen>,
    language: Language,
}

impl DemoApp {
    fn new(cc: &CreationContext<'_>) -> Self {
        apply_cjk_font(&cc.egui_ctx);

        let mut pipeline = WaveformPipeline::new(CHANNELS, SAMPLE_RATE_HZ);
        pipeline.set_time_window(TimeWindow::new(WINDOW_SECS));
        pipeline.set_global_y_scale(YScale::FixedMicrovolts(200.0));

        let gens = (0..CHANNELS)
            .map(|idx| SignalGen {
                freq_hz: 8.0 + idx as f32 * 1.5,
                phase: idx as f32 * 0.6,
                amp_uv: 50.0,
                noise_uv: 10.0,
            })
            .collect();

        Self {
            pipeline,
            started_at: Instant::now(),
            last_ts: 0.0,
            rng: StdRng::from_entropy(),
            gens,
            language: Language::Chinese,
        }
    }

    fn drive_pipeline(&mut self) {
        let dt = 1.0 / SAMPLE_RATE_HZ;
        let target = self.started_at.elapsed().as_secs_f32();

        while self.last_ts + dt <= target {
            let mut frame = Vec::with_capacity(self.gens.len());
            for gen in &self.gens {
                frame.push(gen.sample(self.last_ts, &mut self.rng));
            }
            self.pipeline.ingest_frame(self.last_ts, &frame);
            self.last_ts += dt;
        }
    }

    fn channel_caption(&self, idx: usize, channel: &waveform_pipeline::ChannelView) -> String {
        match self.language {
            Language::English => format!(
                "Channel {} | RMS {:.1} uV | min {:.1} / max {:.1}",
                idx + 1,
                channel.rms_u_v,
                channel.min,
                channel.max
            ),
            Language::Chinese => format!(
                "通道 {} ｜ 均方根 {:.1} uV ｜ 最小 {:.1} / 最大 {:.1}",
                idx + 1,
                channel.rms_u_v,
                channel.min,
                channel.max
            ),
        }
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drive_pipeline();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(self.language.text(UiText::ToggleLanguage))
                    .clicked()
                {
                    self.language = self.language.toggle();
                }
                ui.label(self.language.text(UiText::Subtitle));
            });

            ui.heading(self.language.text(UiText::Title));

            let view = self.pipeline.view();

            for (i, channel) in view.channels.iter().enumerate() {
                ui.separator();
                ui.label(self.channel_caption(i, channel));

                let points = PlotPoints::from_iter(
                    channel
                        .samples
                        .iter()
                        .map(|s| [s.time as f64, s.value as f64]),
                );

                let colors = [
                    egui::Color32::from_rgb(0x5b, 0x8f, 0xff),
                    egui::Color32::from_rgb(0xff, 0x8c, 0x42),
                    egui::Color32::from_rgb(0x54, 0xc7, 0x6b),
                    egui::Color32::from_rgb(0xd1, 0x5b, 0xff),
                ];
                let color = colors[i % colors.len()];

                Plot::new(format!("ch-plot-{}", channel.index))
                    .height(140.0)
                    .allow_drag(false)
                    .allow_zoom(false)
                    .allow_scroll(false)
                    .include_x(-(view.window_secs as f64))
                    .include_x(0.0)
                    .include_y(channel.y_range.0 as f64)
                    .include_y(channel.y_range.1 as f64)
                    .show(ui, |plot_ui| {
                        plot_ui.line(Line::new(points).color(color));
                    });
            }
        });

        ctx.request_repaint(); // continuous streaming
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init();
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Waveform pipeline · 波形管线演示",
        options,
        Box::new(|cc| Box::new(DemoApp::new(cc))),
    )
}

fn apply_cjk_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "cjk".into(),
        egui::FontData::from_static(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../CJK_Font.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "cjk".into());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "cjk".into());
    ctx.set_fonts(fonts);
}

#[derive(Clone, Copy)]
enum Language {
    English,
    Chinese,
}

impl Language {
    fn toggle(self) -> Self {
        match self {
            Language::English => Language::Chinese,
            Language::Chinese => Language::English,
        }
    }

    fn text(self, key: UiText) -> &'static str {
        match (self, key) {
            (Language::English, UiText::Title) => "Waveform pipeline (egui demo)",
            (Language::English, UiText::Subtitle) => {
                "Synthetic data stream; timestamps are relative to the newest sample."
            }
            (Language::English, UiText::ToggleLanguage) => "切换到中文",
            (Language::Chinese, UiText::Title) => "波形管线演示（egui 示例）",
            (Language::Chinese, UiText::Subtitle) => "模拟数据流，时间戳相对于最新采样。",
            (Language::Chinese, UiText::ToggleLanguage) => "Switch to English",
        }
    }
}

#[derive(Clone, Copy)]
enum UiText {
    Title,
    Subtitle,
    ToggleLanguage,
}
