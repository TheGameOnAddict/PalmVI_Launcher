use super::*;

impl App {
    pub(in crate::ui) fn switch_tab(&mut self, ctx: &egui::Context, repo: &str, tab: usize) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(profile_tab_key(repo)), tab));
        if tab == 3 {
            self.show_log = true;
            self.log_expanded = true;
        }
    }
    pub(in crate::ui) fn quick_cards(&mut self, ui: &mut Ui, p: &Project) {
        if self.screenshot_projects.is_some() {
            return;
        }
        let key = project_key(&p.repo);
        let standalone = p.runs_standalone();
        let graphics = self.saved.graphics_for(&p.repo);
        let launch = self.saved.project_launch.get(&key);
        let script_graphics = launch.is_some_and(crate::ProjectLaunch::uses_scarlet_fps);
        let inherited = launch.is_none_or(|s| s.graphics.is_none());
        let method = crate::LaunchMethod::for_project(p, crate::nvidia_rt::supported(&crate::project_script_root(p)));
        let active = launch.and_then(|profile| self.saved.launch_presets.iter().find(|s| Some(&s.name) == profile.selected_preset.as_ref()));
        let preset = launch.map_or_else(|| "Project settings".into(), |profile| launch_presets::setup_name(
            active, profile, if standalone { "" } else { &self.saved.launch_args }, &self.saved.extra_env, self.saved.default_graphics()));
        let save = self
            .saved
            .save_choices
            .get(&key)
            .map(|s| s.0.as_str())
            .unwrap_or("Choose on play")
            .to_owned();
        let mut cards = Vec::new();
        if !standalone {
            cards.push(("Save file", save, "Open save picker", 0));
            cards.push((
                "Graphics",
                graphics_mode(graphics)
                    .map_or("Custom", |i| MODES[i].0)
                    .to_string(),
                if inherited {
                    if script_graphics { "Using script defaults" } else { "Using global settings" }
                } else {
                    "Project override"
                },
                1,
            ));
        }
        cards.push(("Next launch", preset, "Open launch settings", 2));
        ui.columns(cards.len(),|cols|{
            for (ui,(label,value,hint,tab)) in cols.iter_mut().zip(cards){
                let (r,_)=ui.allocate_exact_size(vec2(ui.available_width(),105.0),Sense::hover());
                let arrow=Rect::from_min_size(pos2(r.right()-40.0,r.top()+7.0),vec2(32.0,32.0));
                let body=r;
                let click=ui.interact(body,ui.make_persistent_id((label,"body")),Sense::click());
                ui.painter().add(theme::glass_surface(r,11.0,theme::surface()));
                ui.painter().rect_stroke(r,11.0,Stroke::new(1.0_f32,if click.hovered(){theme::lilac()}else{theme::line()}));
                ui.painter().text(r.min+vec2(15.0,16.0),egui::Align2::LEFT_TOP,format!("{label}  ›"),theme::regular(12.0),theme::muted());
                let text=theme::one_line(ui,&value,theme::strong(17.0),theme::white(),(body.width()-28.0).max(15.0));
                ui.painter().galley(r.min+vec2(15.0,42.0),text,theme::white());
                ui.painter().text(r.min+vec2(15.0,75.0),egui::Align2::LEFT_TOP,hint,theme::regular(11.0),theme::muted());
                if click.clicked(){if tab==0{self.open_save_settings(p.clone());}else{self.switch_tab(ui.ctx(),&p.repo,tab);}}
                let button=ui.interact(arrow,ui.make_persistent_id(("quick-menu",label)),Sense::click()).on_hover_text(format!("Quick change • {label}"));
                ui.painter().rect(arrow,6.0,theme::raised(),Stroke::new(1.0_f32,if button.hovered(){theme::lilac()}else{theme::line()}));
                let mid=arrow.center();ui.painter().add(egui::Shape::line(vec![mid+vec2(-4.0,-2.0),mid+vec2(0.0,2.0),mid+vec2(4.0,-2.0)],Stroke::new(1.3_f32,theme::lilac())));
                let id=button.id.with(label);
                if button.clicked(){ui.memory_mut(|m|m.toggle_popup(id));}
                egui::popup_below_widget(ui,id,&button,egui::PopupCloseBehavior::CloseOnClickOutside,|ui|{
                    ui.set_min_width(180.0);
                    match tab{
                        0=>{
                            let choices_id=id.with("saves");
                            if button.clicked() || ui.ctx().data(|d|d.get_temp::<Vec<(String,String,crate::saves::LaunchSave)>>(choices_id)).is_none(){
                                let choices=SavePicker::new(p.clone()).choices();ui.ctx().data_mut(|d|d.insert_temp(choices_id,choices));
                            }
                            let options=ui.ctx().data(|d|d.get_temp::<Vec<(String,String,crate::saves::LaunchSave)>>(choices_id)).unwrap_or_default();
                            let mut last=String::new();
                            egui::ScrollArea::vertical().max_height(250.0).show(ui,|ui|for (group,name,choice) in options{
                                if group!=last{ui.small(&group);last=group;}
                                let selected=self.saved.save_choices.get(&key).is_some_and(|s|s.1==choice);
                                if ui.selectable_label(selected,&name).clicked(){self.saved.save_choices.insert(key.clone(),(name,choice));ui.memory_mut(|m|m.close_popup());}
                            });
                        }
                        1=>{
                            let launch=self.saved.project_launch.entry(key.clone()).or_default();
                            if ui.selectable_label(inherited,if script_graphics { "Use script defaults" } else { "Use global settings" }).clicked(){launch.graphics=None;ui.memory_mut(|m|m.close_popup());}
                            for (i,(label,series_s,scale,_)) in MODES.iter().enumerate(){
                                if ui.selectable_label(!inherited&&graphics_mode(graphics)==Some(i),*label).clicked(){launch.graphics=Some(GraphicsSettings{series_s:*series_s,render_scale:*scale,..graphics});ui.memory_mut(|m|m.close_popup());}
                            }
                        }
                        _=>{
                            let mut chosen=None;
                            for preset in self.saved.launch_presets.iter().filter(|s|s.repo_script.is_none() && method.accepts(s)){
                                if ui.selectable_label(value==preset.name,&preset.name).clicked(){chosen=Some(preset.clone());}
                            }
                            if let Some(preset)=chosen{
                                let profile=self.saved.project_launch.entry(key.clone()).or_default();
                                if let Err(e)=launch_presets::apply_to_profile(&mut self.saved.launch_presets,profile,&key,preset){self.log_export_status=Some((e,true));}
                                ui.memory_mut(|m|m.close_popup());
                            }
                            if ui.button("Open launch settings…").clicked(){self.switch_tab(ui.ctx(),&p.repo,2);ui.memory_mut(|m|m.close_popup());}
                        }
                    }
                });
            }
        });
    }
}
