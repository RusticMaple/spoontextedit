use std::cell::RefCell;
use std::default;
use std::error::Error;
use std::future::IntoFuture;
use std::path::Path;
use std::rc::Rc;
use std::string::FromUtf8Error;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use egui::{Button, DragValue, RadioButton, Widget};
use egui::Id;

use rfd::AsyncFileDialog;
use thiserror::Error;

use crate::msbt::Msbt;

#[derive(Default)]
pub struct TracebackData{
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
    #[serde(skip)]
    last_location: Arc<Mutex<Option<String>>>,
}

impl Default for SpoonTextEdit {
    fn default() -> Self {
        Self {
            file_data: Default::default(),
            str: Default::default(),
            error_tracebacks: Default::default(),
            selection: Default::default(),
            last_location: Default::default()
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

pub fn exec_traceback<T,F: FnOnce() -> Result<T,Box<dyn Error>>>(error_traceback: Arc<Mutex<TracebackData>>, f: F) -> Option<T>{
    let val = f();

    match val{
        Err(e) => {
            send_error_back(error_traceback, format!("{}",e));
            None
        },
        Ok(v) => {Some(v)}
    }
}

impl SpoonTextEdit {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Default::default()
    }

    fn show_errors(&mut self, ctx: &mut egui::Ui){
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

    async fn open_file(file_data: Arc<Mutex<Option<Msbt>>>, last_location: Arc<Mutex<Option<String>>>, err_traceback: Arc<Mutex<TracebackData>>) {
        let mut file_data = file_data.lock().unwrap();
        let mut dialog = AsyncFileDialog::new();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut lock = last_location.lock().unwrap();
            if let Some(loc) = lock.as_ref(){
                dialog = dialog.set_file_name(loc.clone());
            }
        }


        let handle = dialog.pick_file()
            .await;
        if let Some(handle) = handle {

            #[cfg(not(target_arch = "wasm32"))]
            {
                let mut lock = last_location.lock().unwrap();
                *lock = Some(handle.path().to_str().unwrap().to_owned());
            }

            let content = handle.read().await;



            exec_traceback(err_traceback, move || {
                *file_data = Some(Msbt::load(&content)?);
                Ok(())
            });
        }
    }

    async fn save_file(file_data: Arc<Mutex<Option<Msbt>>>, last_location: Arc<Mutex<Option<String>>>, err_traceback: Arc<Mutex<TracebackData>>) {
        let file_data = file_data.lock().unwrap();
        let Some(file_data) = &*file_data else {
            return;
        };



        let mut dialog = AsyncFileDialog::new();


        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut lock = last_location.lock().unwrap();
            if let Some(loc) = lock.as_ref(){
                dialog = dialog.set_file_name(loc.clone());
            }
        }

        let handle = dialog.save_file()
            .await;;

        if let Some(handle) = handle {

            #[cfg(not(target_arch = "wasm32"))]
            {
                let mut lock = last_location.lock().unwrap();
                *lock = Some(handle.path().to_str().unwrap().to_owned());
            }

            let Some(val) = exec_traceback(err_traceback.clone(), move || {
                Ok(file_data.build()?)
            }) else {
                return;
            };
            //let content = handle.read().await;
            let err = handle.write(&val).await;

            exec_traceback(err_traceback, move ||{
                Ok(err?)
            });
        }
    }
}


impl eframe::App for SpoonTextEdit {
    fn ui(&mut self, ctx: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::MenuBar::new().ui(ui, |ui| {
                if true {
                    ui.menu_button("File", |ui| {
                        if ui.button("Open").clicked() {
                            let file_data = self.file_data.clone();
                            let error_tracebacks = self.error_tracebacks.clone();
                            let last_location = self.last_location.clone();

                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                std::thread::spawn(move || {
                                    futures::executor::block_on(Self::open_file(file_data, last_location, error_tracebacks))
                                });
                            }
                            #[cfg(target_arch = "wasm32")]
                            {

                                wasm_bindgen_futures::spawn_local(Self::open_file(file_data, last_location, error_tracebacks))
                            }

                            ui.close();
                        }

                        if ui.button("Save").clicked() {
                            let file_data = self.file_data.clone();
                            let error_tracebacks = self.error_tracebacks.clone();
                            let last_location = self.last_location.clone();

                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                std::thread::spawn(move || {
                                    futures::executor::block_on(Self::save_file(file_data, last_location, error_tracebacks))
                                });
                            }
                            #[cfg(target_arch = "wasm32")]
                            {

                                wasm_bindgen_futures::spawn_local(Self::save_file(file_data, last_location, error_tracebacks))
                            }

                            ui.close();
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });
        egui::Panel::left("Text Select").show(ctx,|ui|{
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

                    //let mut str = String::new();

                    for attr in &mut file_data.attributes[self.selection as usize]{
                        //str += &format!("{:#04x} ", attr)[2..];

                        
                        DragValue::new(attr).range(0..=256).ui(ui);
                    }

                    //ui.label(str);
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
}
