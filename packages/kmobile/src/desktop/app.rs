//! KMobile Desktop Application
//! Main desktop GUI application for revolutionary mobile hardware emulation and control

use anyhow::Result;
use clap::Parser;
use eframe::egui;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Parser, Debug, Clone)]
#[command(name = "kmobile-desktop")]
#[command(
    about = "KMobile Desktop - Revolutionary hardware emulation and visual control for mobile devices"
)]
pub struct Args {
    #[arg(long, default_value = "3000")]
    pub port: u16,

    #[arg(long, default_value = "localhost")]
    pub host: String,

    #[arg(long)]
    pub device_id: Option<String>,

    #[arg(long)]
    pub fullscreen: bool,

    #[arg(long)]
    pub debug: bool,
}

use crate::desktop::audio::AudioProcessor;
use crate::desktop::computer_vision::ScreenAnalyzer;
use crate::desktop::ui::{AgentPanel, AudioPanel, DevicePanel, HardwarePanel, VisionPanel};
use crate::device_bridge::DeviceBridge;
use crate::hardware_emulator::HardwareEmulator;

pub struct KMobileDesktopApp {
    // Core components
    device_bridge: Arc<RwLock<DeviceBridge>>,
    hardware_emulator: Arc<RwLock<HardwareEmulator>>,
    audio_processor: Arc<RwLock<AudioProcessor>>,
    screen_analyzer: Arc<RwLock<ScreenAnalyzer>>,

    // UI panels
    device_panel: DevicePanel,
    hardware_panel: HardwarePanel,
    audio_panel: AudioPanel,
    vision_panel: VisionPanel,
    agent_panel: AgentPanel,

    // Application state
    connected_device: Option<String>,
    current_screen: Option<egui::TextureHandle>,
    is_recording_audio: bool,
    emulation_active: bool,
    agent_mode: bool,

    // Layout
    left_panel_width: f32,
    right_panel_width: f32,
    main_panel_height: f32,
}

impl KMobileDesktopApp {
    pub async fn new(args: &Args) -> Result<Self> {
        info!("[target] Initializing KMobile Desktop components...");

        // Initialize core components
        let device_bridge = Arc::new(RwLock::new(DeviceBridge::new(&args.host, args.port).await?));

        let hardware_emulator = Arc::new(RwLock::new(HardwareEmulator::new().await?));

        let audio_processor = Arc::new(RwLock::new(AudioProcessor::new().await?));

        let screen_analyzer = Arc::new(RwLock::new(ScreenAnalyzer::new().await?));

        // Initialize UI panels
        let device_panel = DevicePanel::new(device_bridge.clone());
        let hardware_panel = HardwarePanel::new(hardware_emulator.clone());
        let audio_panel = AudioPanel::new(audio_processor.clone());
        let vision_panel = VisionPanel::new(screen_analyzer.clone());
        let agent_panel = AgentPanel::new();

        info!("[ok] KMobile Desktop initialized successfully");

        Ok(Self {
            device_bridge,
            hardware_emulator,
            audio_processor,
            screen_analyzer,
            device_panel,
            hardware_panel,
            audio_panel,
            vision_panel,
            agent_panel,
            connected_device: args.device_id.clone(),
            current_screen: None,
            is_recording_audio: false,
            emulation_active: false,
            agent_mode: false,
            left_panel_width: 300.0,
            right_panel_width: 300.0,
            main_panel_height: 600.0,
        })
    }

    pub async fn run(self) -> Result<()> {
        info!("[launch] Starting KMobile Desktop Application");

        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 800.0])
                .with_min_inner_size([800.0, 600.0]),
            ..Default::default()
        };

        eframe::run_native(
            "KMobile Desktop - Revolutionary Mobile Hardware Emulation",
            options,
            Box::new(|_cc| Ok(Box::new(self))),
        )
        .map_err(|e| anyhow::anyhow!("Failed to start desktop application: {}", e))?;

        Ok(())
    }

    async fn connect_to_device(&mut self, device_id: &str) -> Result<()> {
        info!("[plug] Connecting to device: {}", device_id);

        {
            let mut bridge = self.device_bridge.write().await;
            bridge.connect(device_id).await?;
        } // Drop the lock here

        self.connected_device = Some(device_id.to_string());

        // Start screen mirroring
        self.start_screen_mirroring().await?;

        info!("[ok] Successfully connected to device: {}", device_id);
        Ok(())
    }

    async fn start_screen_mirroring(&mut self) -> Result<()> {
        if let Some(device_id) = &self.connected_device {
            info!("[device] Starting screen mirroring for device: {}", device_id);

            let mut bridge = self.device_bridge.write().await;
            bridge.start_screen_capture().await?;

            // Initialize hardware emulation
            let mut emulator = self.hardware_emulator.write().await;
            emulator.attach_to_device(device_id).await?;

            self.emulation_active = true;
        }
        Ok(())
    }

    async fn toggle_audio_recording(&mut self) -> Result<()> {
        let mut audio = self.audio_processor.write().await;

        if self.is_recording_audio {
            audio.stop_recording().await?;
            info!("[mic] Stopped audio recording");
        } else {
            audio.start_recording().await?;
            info!("[mic] Started audio recording");
        }

        self.is_recording_audio = !self.is_recording_audio;
        Ok(())
    }

    async fn simulate_sensor_input(
        &mut self,
        sensor_type: &str,
        data: serde_json::Value,
    ) -> Result<()> {
        if let Some(device_id) = &self.connected_device {
            let emulator = self.hardware_emulator.read().await;
            emulator
                .simulate_sensor_input(device_id, sensor_type, data)
                .await?;
            debug!("[sensor] Simulated {} sensor input", sensor_type);
        }
        Ok(())
    }

    async fn process_agent_command(&mut self, command: &str) -> Result<String> {
        info!("[bot] Processing agent command: {}", command);

        // Parse natural language command and convert to actions
        let actions = self.parse_agent_command(command).await?;

        let mut results = Vec::new();
        for action in actions {
            let result = self.execute_action(action).await?;
            results.push(result);
        }

        Ok(results.join("\n"))
    }

    async fn parse_agent_command(&self, command: &str) -> Result<Vec<AgentAction>> {
        // TODO: Implement natural language processing for agent commands
        // For now, simple keyword matching
        let mut actions = Vec::new();

        if command.contains("take screenshot") {
            actions.push(AgentAction::TakeScreenshot);
        }

        if command.contains("say") || command.contains("speak") {
            if let Some(text) = extract_speech_text(command) {
                actions.push(AgentAction::Speak(text));
            }
        }

        if command.contains("listen") {
            actions.push(AgentAction::Listen);
        }

        if command.contains("tap") || command.contains("click") {
            if let Some(coords) = extract_coordinates(command) {
                actions.push(AgentAction::Tap(coords));
            }
        }

        Ok(actions)
    }

    async fn execute_action(&mut self, action: AgentAction) -> Result<String> {
        match action {
            AgentAction::TakeScreenshot => {
                let bridge = self.device_bridge.read().await;
                let screenshot = bridge.take_screenshot().await?;
                Ok(format!(
                    "[photo] Screenshot taken: {}x{}",
                    screenshot.width, screenshot.height
                ))
            }

            AgentAction::Speak(text) => {
                let mut audio = self.audio_processor.write().await;
                audio.speak(&text).await?;
                Ok(format!("[tts] Spoke: '{text}'"))
            }

            AgentAction::Listen => {
                let mut audio = self.audio_processor.write().await;
                let transcript = audio.listen_and_transcribe().await?;
                Ok(format!("[listen] Heard: '{transcript}'"))
            }

            AgentAction::Tap(coords) => {
                let bridge = self.device_bridge.read().await;
                bridge.tap(coords.0, coords.1).await?;
                Ok(format!("[tap] Tapped at ({}, {})", coords.0, coords.1))
            }

            AgentAction::SimulateSensor { sensor_type, data } => {
                self.simulate_sensor_input(&sensor_type, data).await?;
                Ok(format!("[sensor] Simulated {sensor_type} sensor"))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentAction {
    TakeScreenshot,
    Speak(String),
    Listen,
    Tap((i32, i32)),
    SimulateSensor {
        sensor_type: String,
        data: serde_json::Value,
    },
}

impl eframe::App for KMobileDesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request repaint for smooth animations
        ctx.request_repaint();

        // Top menu bar
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Device", |ui| {
                    if ui.button("[plug] Connect Device").clicked() {
                        // Open device connection dialog
                    }
                    if ui.button("[device] Disconnect").clicked() {
                        // Disconnect current device
                    }
                    ui.separator();
                    if ui.button("[loop] Refresh Devices").clicked() {
                        // Refresh device list
                    }
                });

                ui.menu_button("Hardware", |ui| {
                    ui.checkbox(&mut self.emulation_active, "[hw-emu] Enable Emulation");
                    ui.separator();
                    if ui.button("[sensor] GPS Simulation").clicked() {
                        // Open GPS simulation dialog
                    }
                    if ui.button("[vol-up] Audio Routing").clicked() {
                        // Open audio routing dialog
                    }
                });

                ui.menu_button("Agent", |ui| {
                    ui.checkbox(&mut self.agent_mode, "[bot] Agent Mode");
                    ui.separator();
                    if ui.button("[target] Run Agent Task").clicked() {
                        // Open agent task dialog
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(device) = &self.connected_device {
                        ui.label(format!("[device] Connected: {device}"));
                    } else {
                        ui.label("[err] No device connected");
                    }
                });
            });
        });

        // Left panel - Device and Hardware controls
        egui::SidePanel::left("left_panel")
            .resizable(true)
            .default_width(self.left_panel_width)
            .show(ctx, |ui| {
                ui.heading("[hw-emu] Hardware Control");

                ui.collapsing("[device] Device Connection", |ui| {
                    self.device_panel.show(ui);
                });

                ui.collapsing("[tools] Hardware Emulation", |ui| {
                    self.hardware_panel.show(ui);
                });

                ui.collapsing("[audio] Audio Processing", |ui| {
                    self.audio_panel.show(ui);
                });
            });

        // Right panel - Vision and Agent controls
        egui::SidePanel::right("right_panel")
            .resizable(true)
            .default_width(self.right_panel_width)
            .show(ctx, |ui| {
                ui.heading("[bot] AI Control");

                ui.collapsing("[vision] Computer Vision", |ui| {
                    self.vision_panel.show(ui);
                });

                ui.collapsing("[ai] Agent Interface", |ui| {
                    self.agent_panel.show(ui);
                });
            });

        // Central panel - Device screen mirroring
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("[device] Device Screen");

            if let Some(texture) = &self.current_screen {
                let available_size = ui.available_size();
                let aspect_ratio = 16.0 / 9.0; // Typical phone aspect ratio

                let display_size = if available_size.x / available_size.y > aspect_ratio {
                    // Wide layout - fit height
                    egui::Vec2::new(available_size.y * aspect_ratio, available_size.y)
                } else {
                    // Tall layout - fit width
                    egui::Vec2::new(available_size.x, available_size.x / aspect_ratio)
                };

                let response = ui.add(
                    egui::Image::from_texture(texture)
                        .fit_to_exact_size(display_size)
                        .sense(egui::Sense::click_and_drag()),
                );

                // Handle touch interactions
                if response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let relative_pos = pos - response.rect.min;
                        let screen_x = (relative_pos.x / display_size.x * 1080.0) as i32;
                        let screen_y = (relative_pos.y / display_size.y * 1920.0) as i32;

                        info!(
                            "[tap] User tapped at screen coordinates: ({}, {})",
                            screen_x, screen_y
                        );
                        // TODO: Send tap command to device
                    }
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("[device] Connect a device to see its screen");
                    ui.label("[plug] Use the left panel to connect to a device");
                });
            }
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.emulation_active {
                    ui.label("[green] Hardware Emulation: Active");
                } else {
                    ui.label("[red] Hardware Emulation: Inactive");
                }

                ui.separator();

                if self.is_recording_audio {
                    ui.label("[mic] Audio: Recording");
                } else {
                    ui.label("[mic] Audio: Idle");
                }

                ui.separator();

                if self.agent_mode {
                    ui.label("[bot] Agent Mode: Enabled");
                } else {
                    ui.label("[bot] Agent Mode: Disabled");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("FPS: {:.1}", ctx.input(|i| i.stable_dt).recip()));
                });
            });
        });
    }
}

// Helper functions for command parsing
fn extract_speech_text(command: &str) -> Option<String> {
    // Extract text between quotes or after "say"/"speak"
    if let Some(start) = command.find('"') {
        if let Some(end) = command[start + 1..].find('"') {
            return Some(command[start + 1..start + 1 + end].to_string());
        }
    }

    // Fallback: everything after "say" or "speak"
    for keyword in ["say ", "speak "] {
        if let Some(pos) = command.find(keyword) {
            return Some(command[pos + keyword.len()..].trim().to_string());
        }
    }

    None
}

fn extract_coordinates(command: &str) -> Option<(i32, i32)> {
    // Look for coordinate patterns like "(123, 456)" or "123,456"
    use regex::Regex;
    let re = Regex::new(r"(\d+),?\s*(\d+)").unwrap();

    if let Some(captures) = re.captures(command) {
        if let (Ok(x), Ok(y)) = (captures[1].parse(), captures[2].parse()) {
            return Some((x, y));
        }
    }

    None
}
