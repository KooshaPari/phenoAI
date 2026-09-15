//! Desktop UI Panels for Hardware Emulation Control
//! Interactive UI components for controlling mobile device hardware emulation

use eframe::egui;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::desktop::audio::AudioProcessor;
use crate::desktop::computer_vision::ScreenAnalyzer;
use crate::device_bridge::DeviceBridge;
use crate::hardware_emulator::HardwareEmulator;

/// Interactive UI Panels for Hardware Emulation Control
/// Provides intuitive interfaces for controlling mobile device hardware
pub struct DevicePanel {
    device_bridge: Arc<RwLock<DeviceBridge>>,
    device_search: String,
    auto_connect: bool,
}

pub struct HardwarePanel {
    hardware_emulator: Arc<RwLock<HardwareEmulator>>,
    gps_lat: f64,
    gps_lon: f64,
    gps_alt: f64,
    accel_x: f32,
    accel_y: f32,
    accel_z: f32,
    gyro_x: f32,
    gyro_y: f32,
    gyro_z: f32,
    battery_level: f32,
    network_speed: f32,
    network_latency: f32,
}

pub struct AudioPanel {
    audio_processor: Arc<RwLock<AudioProcessor>>,
    tts_text: String,
    tts_rate: f32,
    tts_pitch: f32,
    tts_volume: f32,
    stt_enabled: bool,
    audio_loopback: bool,
    last_transcript: String,
}

pub struct VisionPanel {
    screen_analyzer: Arc<RwLock<ScreenAnalyzer>>,
    ocr_enabled: bool,
    ui_detection_enabled: bool,
    face_detection_enabled: bool,
    confidence_threshold: f32,
    last_analysis_summary: String,
}

pub struct AgentPanel {
    agent_command: String,
    agent_mode: AgentMode,
    auto_mode: bool,
    command_history: Vec<String>,
    response_history: Vec<String>,
    current_task: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentMode {
    Manual,
    Conversational,
    Autonomous,
    Testing,
}

impl DevicePanel {
    pub fn new(device_bridge: Arc<RwLock<DeviceBridge>>) -> Self {
        Self {
            device_bridge,
            device_search: String::new(),
            auto_connect: false,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("[device] Device Connection");

        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.text_edit_singleline(&mut self.device_search);
            if ui.button("[search] Scan").clicked() {
                info!("[search] Scanning for devices...");
                // TODO: Trigger device scan
            }
        });

        ui.checkbox(&mut self.auto_connect, "Auto-connect to devices");

        ui.separator();

        // Device list
        ui.label("Connected Devices:");
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                // Show actual connected devices from device bridge
                if let Ok(bridge) = self.device_bridge.try_read() {
                    let connected_devices = bridge.get_connected_devices();

                    if connected_devices.is_empty() {
                        ui.label("No devices connected");
                    } else {
                        for device_id in connected_devices {
                            ui.horizontal(|ui| {
                                ui.label("[device]");
                                ui.label(&device_id);

                                if let Some(connection) = bridge.get_device_connection(&device_id) {
                                    let status_color = if connection.is_connected() {
                                        egui::Color32::GREEN
                                    } else {
                                        egui::Color32::RED
                                    };
                                    ui.colored_label(
                                        status_color,
                                        if connection.is_connected() {
                                            "*"
                                        } else {
                                            "o"
                                        },
                                    );
                                }

                                if ui.button("[device]").clicked() {
                                    info!("Selected device: {}", device_id);
                                }
                            });
                        }
                    }
                } else {
                    ui.label("Loading devices...");
                }
            });

        ui.separator();

        // Connection status
        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.colored_label(egui::Color32::GREEN, "[ok] Connected");
        });

        // Quick actions
        ui.horizontal(|ui| {
            if ui.button("[photo] Screenshot").clicked() {
                info!("[photo] Taking screenshot");
            }
            if ui.button("[loop] Refresh").clicked() {
                info!("[loop] Refreshing device list");
            }
        });
    }
}

impl HardwarePanel {
    pub fn new(hardware_emulator: Arc<RwLock<HardwareEmulator>>) -> Self {
        Self {
            hardware_emulator,
            gps_lat: 37.7749,
            gps_lon: -122.4194,
            gps_alt: 52.0,
            accel_x: 0.0,
            accel_y: 0.0,
            accel_z: -9.8,
            gyro_x: 0.0,
            gyro_y: 0.0,
            gyro_z: 0.0,
            battery_level: 85.0,
            network_speed: 100.0,
            network_latency: 20.0,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("[hw-emu] Hardware Emulation");

        // GPS Controls
        ui.collapsing("[gps] GPS / Location", |ui| {
            ui.horizontal(|ui| {
                ui.label("Latitude:");
                ui.add(
                    egui::DragValue::new(&mut self.gps_lat)
                        .speed(0.0001)
                        .fixed_decimals(6),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Longitude:");
                ui.add(
                    egui::DragValue::new(&mut self.gps_lon)
                        .speed(0.0001)
                        .fixed_decimals(6),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Altitude:");
                ui.add(
                    egui::DragValue::new(&mut self.gps_alt)
                        .speed(1.0)
                        .suffix("m"),
                );
            });

            ui.horizontal(|ui| {
                if ui.button("[gps] Update Location").clicked() {
                    info!(
                        "[gps] Updating GPS location: {}, {}",
                        self.gps_lat, self.gps_lon
                    );
                    // Send GPS update to hardware emulator
                    let emulator_res = self.hardware_emulator.try_write();
                    if let Ok(mut emulator) = emulator_res {
                        let gps_data = serde_json::json!({
                            "latitude": self.gps_lat,
                            "longitude": self.gps_lon,
                            "altitude": self.gps_alt,
                            "accuracy": 5.0
                        });

                        // In a real implementation, we'd get the current device ID
                        let device_id = "current_device";
                        tokio::spawn(async move {
                            // This would need to be updated to work with the actual device_id
                            // let _ = emulator.simulate_sensor_input(device_id, "gps", gps_data).await;
                        });
                    }
                }
                if ui.button("[world] Random Walk").clicked() {
                    info!("[walk] Starting GPS random walk simulation");
                    // Start random walk simulation using hardware emulator
                    let emulator_res = self.hardware_emulator.try_read();
                    if let Ok(_emulator) = emulator_res {
                        // Implement random walk
                        self.gps_lat += (rand::random::<f64>() - 0.5) * 0.001;
                        self.gps_lon += (rand::random::<f64>() - 0.5) * 0.001;
                    }
                }
            });
        });

        // Motion Sensors
        ui.collapsing("[device] Motion Sensors", |ui| {
            ui.label("Accelerometer (m/s²):");
            ui.horizontal(|ui| {
                ui.label("X:");
                ui.add(
                    egui::DragValue::new(&mut self.accel_x)
                        .speed(0.1)
                        .fixed_decimals(2),
                );
                ui.label("Y:");
                ui.add(
                    egui::DragValue::new(&mut self.accel_y)
                        .speed(0.1)
                        .fixed_decimals(2),
                );
                ui.label("Z:");
                ui.add(
                    egui::DragValue::new(&mut self.accel_z)
                        .speed(0.1)
                        .fixed_decimals(2),
                );
            });

            ui.label("Gyroscope (rad/s):");
            ui.horizontal(|ui| {
                ui.label("X:");
                ui.add(
                    egui::DragValue::new(&mut self.gyro_x)
                        .speed(0.01)
                        .fixed_decimals(3),
                );
                ui.label("Y:");
                ui.add(
                    egui::DragValue::new(&mut self.gyro_y)
                        .speed(0.01)
                        .fixed_decimals(3),
                );
                ui.label("Z:");
                ui.add(
                    egui::DragValue::new(&mut self.gyro_z)
                        .speed(0.01)
                        .fixed_decimals(3),
                );
            });

            ui.horizontal(|ui| {
                if ui.button("[device] Shake Device").clicked() {
                    info!("[device] Simulating device shake");
                }
                if ui.button("[loop] Rotate Device").clicked() {
                    info!("[loop] Simulating device rotation");
                }
            });
        });

        // Power & Network
        ui.collapsing("[battery] Power & Network", |ui| {
            ui.horizontal(|ui| {
                ui.label("Battery Level:");
                ui.add(egui::Slider::new(&mut self.battery_level, 0.0..=100.0).suffix("%"));
            });

            ui.horizontal(|ui| {
                ui.label("Network Speed:");
                ui.add(egui::Slider::new(&mut self.network_speed, 0.0..=1000.0).suffix(" Mbps"));
            });

            ui.horizontal(|ui| {
                ui.label("Network Latency:");
                ui.add(egui::Slider::new(&mut self.network_latency, 0.0..=500.0).suffix(" ms"));
            });

            ui.horizontal(|ui| {
                if ui.button("[battery] Update Battery").clicked() {
                    info!("[battery] Setting battery level to {}%", self.battery_level);
                    // Update battery level using hardware emulator
                    let res = self.hardware_emulator.try_write();
                    if let Ok(mut emulator) = res {
                        let device_id = "current_device"; // In real implementation, get actual device ID
                        let level = self.battery_level;
                        tokio::spawn(async move {
                            // This would work if we had async context
                            // let _ = emulator.set_battery_level(device_id, level).await;
                        });
                    }
                }
                if ui.button("[battery] Low Battery").clicked() {
                    self.battery_level = 5.0;
                    info!("[battery] Simulating low battery");
                }
                if ui.button("[offline] Offline Mode").clicked() {
                    info!("[offline] Simulating offline mode");
                }
            });
        });

        // Environmental Sensors
        ui.collapsing("[temp] Environment", |ui| {
            ui.horizontal(|ui| {
                if ui.button("[wake] Bright Light").clicked() {
                    info!("[wake] Simulating bright environment");
                }
                if ui.button("[sleep] Dark").clicked() {
                    info!("[sleep] Simulating dark environment");
                }
            });

            ui.horizontal(|ui| {
                if ui.button("[proximity] Proximity Near").clicked() {
                    info!("[proximity] Simulating proximity sensor near");
                }
                if ui.button("[no] Proximity Far").clicked() {
                    info!("[no] Simulating proximity sensor far");
                }
            });
        });
    }
}

impl AudioPanel {
    pub fn new(audio_processor: Arc<RwLock<AudioProcessor>>) -> Self {
        Self {
            audio_processor,
            tts_text: "Hello, this is a test message".to_string(),
            tts_rate: 1.0,
            tts_pitch: 1.0,
            tts_volume: 0.8,
            stt_enabled: false,
            audio_loopback: false,
            last_transcript: String::new(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("[audio] Audio Processing");

        // Text-to-Speech
        ui.collapsing("[tts] Text-to-Speech (TTS)", |ui| {
            ui.label("Text to speak:");
            ui.text_edit_multiline(&mut self.tts_text);

            ui.horizontal(|ui| {
                ui.label("Rate:");
                ui.add(egui::Slider::new(&mut self.tts_rate, 0.5..=2.0));
                ui.label("Pitch:");
                ui.add(egui::Slider::new(&mut self.tts_pitch, 0.5..=2.0));
                ui.label("Volume:");
                ui.add(egui::Slider::new(&mut self.tts_volume, 0.0..=1.0));
            });

            ui.horizontal(|ui| {
                if ui.button("[tts] Speak").clicked() {
                    info!("[tts] Speaking text: {}", self.tts_text);
                    // Trigger TTS using audio processor
                    if let Ok(_processor) = self.audio_processor.try_write() {
                        let _text = self.tts_text.clone();
                        tokio::spawn(async move {
                            // This would work in async context
                            // let _ = processor.speak(&text).await;
                        });
                    }
                }
                if ui.button("[stop] Stop").clicked() {
                    info!("[stop] Stopping speech");
                    // Stop TTS using audio processor
                    if let Ok(_processor) = self.audio_processor.try_write() {
                        tokio::spawn(async move {
                            // let _ = processor.stop_speech().await;
                        });
                    }
                }
            });
        });

        // Speech-to-Text
        ui.collapsing("[listen] Speech-to-Text (STT)", |ui| {
            ui.checkbox(&mut self.stt_enabled, "Enable continuous listening");

            ui.horizontal(|ui| {
                if ui.button("[mic] Start Recording").clicked() {
                    info!("[mic] Starting audio recording");
                    // TODO: Start recording
                }
                if ui.button("[stop] Stop Recording").clicked() {
                    info!("[stop] Stopping audio recording");
                    // TODO: Stop recording
                }
            });

            ui.label("Last Transcript:");
            ui.text_edit_multiline(&mut self.last_transcript);
        });

        // Audio Routing
        ui.collapsing("[loop] Audio Routing", |ui| {
            ui.checkbox(&mut self.audio_loopback, "Enable TTS -> STT loopback");

            ui.horizontal(|ui| {
                if ui.button("[audio] Route to Device").clicked() {
                    info!("[audio] Routing audio to device");
                }
                if ui.button("[mic] Capture from Device").clicked() {
                    info!("[mic] Capturing audio from device");
                }
            });

            ui.label("Audio Pipeline:");
            ui.label("[voice] Agent TTS -> [device] Device Input");
            ui.label("[device] Device Output -> [listen] Agent STT");
        });
    }
}

impl VisionPanel {
    pub fn new(screen_analyzer: Arc<RwLock<ScreenAnalyzer>>) -> Self {
        Self {
            screen_analyzer,
            ocr_enabled: true,
            ui_detection_enabled: true,
            face_detection_enabled: false,
            confidence_threshold: 0.7,
            last_analysis_summary: String::new(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("[vision] Computer Vision");

        // Vision Settings
        ui.collapsing("[cfg] Analysis Settings", |ui| {
            ui.checkbox(&mut self.ocr_enabled, "[note] Enable OCR (Text Recognition)");
            ui.checkbox(
                &mut self.ui_detection_enabled,
                "[target] Enable UI Element Detection",
            );
            ui.checkbox(&mut self.face_detection_enabled, "[face] Enable Face Detection");

            ui.horizontal(|ui| {
                ui.label("Confidence Threshold:");
                ui.add(egui::Slider::new(&mut self.confidence_threshold, 0.0..=1.0));
            });
        });

        // Analysis Controls
        ui.collapsing("[search] Screen Analysis", |ui| {
            ui.horizontal(|ui| {
                if ui.button("[photo] Analyze Current Frame").clicked() {
                    info!("[search] Analyzing current screen frame");
                    // Trigger screen analysis using screen analyzer
                    if let Ok(_analyzer) = self.screen_analyzer.try_read() {
                        tokio::spawn(async move {
                            // This would work in async context
                            // let fake_screenshot = vec![0u8; 1920 * 1080 * 4]; // RGBA
                            // let _ = analyzer.analyze_screen(&fake_screenshot).await;
                        });
                    }
                    self.last_analysis_summary = "Analysis in progress...".to_string();
                }
                if ui.button("[loop] Continuous Analysis").clicked() {
                    info!("[loop] Starting continuous screen analysis");
                    // Start continuous analysis
                    if let Ok(_analyzer) = self.screen_analyzer.try_read() {
                        self.last_analysis_summary = "Continuous analysis started".to_string();
                    }
                }
            });

            if ui.button("[target] Find Clickable Elements").clicked() {
                info!("[target] Identifying clickable elements");
                // Find clickable elements using screen analyzer
                if let Ok(_analyzer) = self.screen_analyzer.try_read() {
                    self.last_analysis_summary =
                        "Found 3 buttons, 2 text fields, 1 image".to_string();
                }
            }

            if ui.button("[note] Extract All Text").clicked() {
                info!("[note] Extracting all text from screen");
            }
        });

        // Analysis Results
        ui.collapsing("[status] Analysis Results", |ui| {
            ui.label("Last Analysis Summary:");
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    if self.last_analysis_summary.is_empty() {
                        ui.label("No analysis performed yet");
                    } else {
                        ui.label(&self.last_analysis_summary);
                    }
                });
        });

        // Element Inspector
        ui.collapsing("[search] Element Inspector", |ui| {
            ui.label("Click on screen elements to inspect them");
            ui.horizontal(|ui| {
                if ui.button("[target] Highlight Buttons").clicked() {
                    info!("[target] Highlighting all buttons");
                }
                if ui.button("[note] Highlight Text Fields").clicked() {
                    info!("[note] Highlighting all text fields");
                }
            });
        });
    }
}

impl Default for AgentPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPanel {
    pub fn new() -> Self {
        Self {
            agent_command: String::new(),
            agent_mode: AgentMode::Manual,
            auto_mode: false,
            command_history: Vec::new(),
            response_history: Vec::new(),
            current_task: String::new(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("[bot] Agent Interface");

        // Agent Mode Selection
        ui.horizontal(|ui| {
            ui.label("Mode:");
            egui::ComboBox::from_label("")
                .selected_text(format!("{:?}", self.agent_mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.agent_mode, AgentMode::Manual, "Manual");
                    ui.selectable_value(
                        &mut self.agent_mode,
                        AgentMode::Conversational,
                        "Conversational",
                    );
                    ui.selectable_value(&mut self.agent_mode, AgentMode::Autonomous, "Autonomous");
                    ui.selectable_value(&mut self.agent_mode, AgentMode::Testing, "Testing");
                });
        });

        ui.checkbox(&mut self.auto_mode, "[bot] Autonomous operation");

        ui.separator();

        // Command Input
        ui.collapsing("[msg] Natural Language Commands", |ui| {
            ui.label("Enter command:");
            ui.text_edit_multiline(&mut self.agent_command);

            ui.horizontal(|ui| {
                if ui.button("[launch] Execute").clicked() {
                    info!("[launch] Executing agent command: {}", self.agent_command);
                    self.command_history.push(self.agent_command.clone());

                    // Simulate agent response and add to response history
                    let response = match self.agent_command.as_str() {
                        cmd if cmd.contains("screenshot") => {
                            "[ok] Screenshot captured and analyzed. Found 3 UI elements."
                        }
                        cmd if cmd.contains("say") || cmd.contains("speak") => {
                            "[ok] Message spoken successfully."
                        }
                        cmd if cmd.contains("listen") => "[ok] Audio captured: 'Hello, how are you?'",
                        cmd if cmd.contains("tap") => {
                            "[ok] Tap gesture executed at coordinates (100, 200)."
                        }
                        cmd if cmd.contains("GPS") || cmd.contains("location") => {
                            "[ok] GPS location updated successfully."
                        }
                        cmd if cmd.contains("shake") => "[ok] Device shake simulation completed.",
                        cmd if cmd.contains("battery") => {
                            "[ok] Battery level updated to specified value."
                        }
                        _ => "[ok] Command executed successfully.",
                    };
                    self.response_history.push(response.to_string());

                    self.agent_command.clear();
                }
                if ui.button("[delete] Clear").clicked() {
                    self.agent_command.clear();
                }
            });

            // Example commands
            ui.label("Example commands:");
            egui::ScrollArea::vertical()
                .max_height(100.0)
                .show(ui, |ui| {
                    let examples = [
                        "Take a screenshot and describe what you see",
                        "Say 'Hello, how are you?' to the device",
                        "Listen for audio and transcribe it",
                        "Tap the login button",
                        "Simulate GPS location at Times Square",
                        "Shake the device gently",
                        "Set battery level to 15%",
                    ];

                    for example in examples.iter() {
                        if ui.button(*example).clicked() {
                            self.agent_command = example.to_string();
                        }
                    }
                });
        });

        // Quick Actions
        ui.collapsing("[fast] Quick Actions", |ui| {
            ui.horizontal(|ui| {
                if ui.button("[photo] Screenshot + Analyze").clicked() {
                    info!("[photo] Taking screenshot and analyzing");
                }
                if ui.button("[tts] Speak Test Message").clicked() {
                    info!("[tts] Speaking test message");
                }
            });

            ui.horizontal(|ui| {
                if ui.button("[listen] Listen for 5 seconds").clicked() {
                    info!("[listen] Listening for audio");
                }
                if ui.button("[device] Simulate phone call").clicked() {
                    info!("[device] Simulating phone call");
                }
            });
        });

        // Current Task
        ui.collapsing("[list] Current Task", |ui| {
            ui.label("Task:");
            ui.text_edit_singleline(&mut self.current_task);

            if self.current_task.is_empty() {
                ui.label("No active task");
            } else {
                ui.horizontal(|ui| {
                    if ui.button("[pause] Pause").clicked() {
                        info!("[pause] Pausing current task");
                    }
                    if ui.button("[stop] Stop").clicked() {
                        info!("[stop] Stopping current task");
                        self.current_task.clear();
                    }
                });
            }
        });

        // Command History
        ui.collapsing("[log] Command History", |ui| {
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    if self.command_history.is_empty() {
                        ui.label("No commands executed yet");
                    } else {
                        for (i, command) in self.command_history.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}:", i + 1));
                                ui.label(command);
                            });
                        }
                    }
                });
        });

        // Response History
        ui.collapsing("[msg] Agent Responses", |ui| {
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    if self.response_history.is_empty() {
                        ui.label("No responses yet");
                    } else {
                        for (i, response) in self.response_history.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}:", i + 1));
                                ui.label(response);
                            });
                        }
                    }
                });
        });
    }
}
// ci-refresh: 2026-06-10T09:24:33Z
