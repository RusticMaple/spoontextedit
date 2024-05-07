use std::cell::RefCell;
use std::default;
use std::error::Error;
use std::future::IntoFuture;
use std::rc::Rc;
use std::string::FromUtf8Error;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use egui::{Button, RadioButton, Widget};
use egui::WidgetType::SelectableLabel;

use rfd::AsyncFileDialog;
use thiserror::Error;

use crate::msbt::Msbt;

#[derive(Default)]
struct TracebackData{
    traceback_counter: u32,
    tracebacks: Vec<(String,u32)>,
}


/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct SpoonTextEdit {
    #[serde(skip)]
    file_data: Arc<Mutex<Option<Msbt>>>,
    #[serde(skip)]
    error_tracebacks: Arc<Mutex<TracebackData>>,
    #[serde(skip)]
    str: String,
    #[serde(skip)]
    selection: u32,
}

impl Default for SpoonTextEdit {
    fn default() -> Self {
        Self {
            file_data: Default::default(),
            str: Default::default(),
            error_tracebacks: Default::default(),
            selection: Default::default()
        }
    }
}

pub fn send_error_back(error_traceback: Arc<Mutex<TracebackData>>, error: String){
    let mut traceback = error_traceback.lock().unwrap();

    let current_counter = traceback.traceback_counter;
    eprintln!("An Error occurred: {}", error);
    traceback.tracebacks.push((error, current_counter));

    traceback.traceback_counter += 1;

}

pub fn exec_traceback<F: FnOnce() -> Result<(),Box<dyn Error>>>(error_traceback: Arc<Mutex<TracebackData>>, f: F){
    match (f)(){
        Err(e) => {
            send_error_back(error_traceback, format!("{}",e));
        },
        _ => {}
    }
}

impl SpoonTextEdit {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }

    fn show_errors(&mut self, ctx: &egui::Context){
        let mut tracebacks = self.error_tracebacks.lock().unwrap();

        for traceback in &mut ((*tracebacks).tracebacks){

            let mut keep_error = true;
            egui::Window::new("An Error Occurred")
                .id(format!("Error: {:?}", traceback.1).into())
                .collapsible(false)
                .open(&mut keep_error)
                .show(ctx, |ui| {
                    ui.label(&traceback.0);
                    if ui.button("Close").clicked(){
                        traceback.0 = String::from("");
                    }
                });
            if !keep_error{
                traceback.0 = String::from("");
            }
        }

        if let Some(del_traceback) = tracebacks.tracebacks.iter().position(|t| t.0 == ""){
            tracebacks.tracebacks.remove(del_traceback);
        }
    }

    async fn open_file(file_data: Arc<Mutex<Option<Msbt>>>, err_traceback: Arc<Mutex<TracebackData>>) {
        let mut file_data = file_data.lock().unwrap();
        let handle = AsyncFileDialog::new()
            .pick_file()
            .await;
        if let Some(handle) = handle {

            let content = handle.read().await;

            exec_traceback(err_traceback, move || {
                *file_data = Some(Msbt::load(&content)?);
                Ok(())
            })
        }
    }
}


impl eframe::App for SpoonTextEdit {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                if true {
                    ui.menu_button("File", |ui| {
                        if ui.button("Open").clicked() {
                            let file_data = self.file_data.clone();
                            let error_tracebacks = self.error_tracebacks.clone();

                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                std::thread::spawn(move || {
                                    futures::executor::block_on(Self::open_file(file_data, error_tracebacks))
                                });
                            }
                            #[cfg(target_arch = "wasm32")]
                            {

                                wasm_bindgen_futures::spawn_local(Self::open_file(file_data, error_tracebacks))
                            }

                            ui.close_menu();
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_dark_light_mode_buttons(ui);
            });
        });
        egui::SidePanel::left("Text Select").show(ctx,|ui|{
            egui::ScrollArea::vertical().show(ui,|ui|{
                if let Ok(file_data) = self.file_data.try_lock() {
                    if let Some(file_data) = (*file_data).as_ref() {
                        for label in &file_data.labels {
                            ui.selectable_value(&mut self.selection, label.index, format!("{}: {}",label.name, label.index));
                        }
                    }
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui|{
            if let Ok(mut file_data) = self.file_data.try_lock() {
                if let Some(file_data) = (*file_data).as_mut() {
                    if self.selection as usize >= file_data.texts.len(){
                        self.selection = 0;
                    }
                    
                    ui.text_edit_multiline(&mut file_data.texts[self.selection as usize]);
                }
            }
        });
        
        if let Err(_) = self.file_data.try_lock(){
            // Ignore all errors for now on try lock (this might not be the best idea 
            // as we may want to show something to the user if the mutex is poisoned)
            ctx.request_repaint();
        }
        
        // Show errors
        self.show_errors(ctx);
    }
    
    

    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}
