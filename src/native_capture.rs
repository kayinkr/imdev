use eframe::egui::{self, epaint::Primitive};
use serde_json::json;
use std::{fs, path::Path};

fn dump(ctx: &egui::Context, app: &mut crate::ui::ImdevApp, dir: &Path, width: f32, height: f32, actions: &[&str]) {
    fs::create_dir_all(dir).unwrap();
    let mut textures = Vec::new();
    let mut meshes = Vec::new();
    let mut labels:Vec<(String,egui::Pos2)>=Vec::new();
    let mut pointer=egui::Pos2::ZERO;
    let frames=actions.len()*2+3;
    for frame in 0..frames {
        let mut events=Vec::new();
        if frame>0 && frame<=actions.len()*2 {
            let pressed=frame%2==1;
            if pressed {
                let target=actions[(frame-1)/2];
                pointer=labels.iter().find(|(text,_)|text==target).unwrap_or_else(||panic!("Missing actual native widget: {target}")).1;
            }
            events.push(egui::Event::PointerMoved(pointer));
            events.push(egui::Event::PointerButton{pos:pointer,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE});
        }
        let output = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))), time: Some(frame as f64 * 0.1), events, ..Default::default() }, |ctx| app.show(ctx));
        fn collect(shape:&egui::epaint::Shape,labels:&mut Vec<(String,egui::Pos2)>){match shape{egui::epaint::Shape::Text(t)=>labels.push((t.galley.text().to_string(),t.pos+t.galley.rect.center().to_vec2())),egui::epaint::Shape::Vec(shapes)=>for shape in shapes{collect(shape,labels);},_=>{}}}
        labels.clear();for clipped in &output.shapes{collect(&clipped.shape,&mut labels);}
        for (id, delta) in &output.textures_delta.set {
            let egui::ImageData::Color(image) = &delta.image;
            let name = format!("texture-{}-{}.rgba", frame, textures.len());
            let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            fs::write(dir.join(&name), pixels).unwrap();
            let id = match id { egui::TextureId::Managed(n) => *n, egui::TextureId::User(n) => *n + 1_000_000 };
            textures.push(json!({"id":id,"size":image.size,"pos":delta.pos,"file":name}));
        }
        if frame == frames-1 {
            for clipped in ctx.tessellate(output.shapes, output.pixels_per_point) {
                if let Primitive::Mesh(mesh) = clipped.primitive {
                    let id = match mesh.texture_id { egui::TextureId::Managed(n) => n, egui::TextureId::User(n) => n + 1_000_000 };
                    meshes.push(json!({"texture":id,"clip":[clipped.clip_rect.min.x,clipped.clip_rect.min.y,clipped.clip_rect.max.x,clipped.clip_rect.max.y],"vertices":mesh.vertices.iter().map(|v|json!([v.pos.x,v.pos.y,v.uv.x,v.uv.y,v.color.to_array()])).collect::<Vec<_>>(),"indices":mesh.indices}));
                }
            }
        }
    }
    assert!(!meshes.is_empty());
    assert!(!labels.iter().any(|(label,_)|label.contains("synthetic-preview-token")),"Session token must never enter actual painted labels");
    assert!(textures.iter().any(|t|t["id"]==0));
    if actions.contains(&"Pretty JSON") {assert!(labels.iter().any(|(text,_)|text.contains("\"service\": \"Imdev loopback mock\"")),"Actual Pretty JSON pointer action must format the local response");}
    fs::write(dir.join("frame.json"),serde_json::to_vec(&json!({"width":width as u32,"height":height as u32,"textures":textures,"meshes":meshes,"actions":actions,"evidence":"Actual native egui tessellation, software preview; not a Windows OS screenshot"})).unwrap()).unwrap();
}

#[test]
fn export_actual_local_http_response_frame() {
    let Ok(base)=std::env::var("IMDEV_CAPTURE_DIR") else {return;};
    let runtime=tokio::runtime::Runtime::new().unwrap();
    let (request,response)=runtime.block_on(async {
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move {
            let(mut stream,_)=listener.accept().await.unwrap();
            let mut bytes=Vec::new();let mut buffer=[0u8;4096];let header_end;
            loop{let n=stream.read(&mut buffer).await.unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);if let Some(at)=bytes.windows(4).position(|w|w==b"\r\n\r\n"){header_end=at+4;break;}}
            let headers=String::from_utf8_lossy(&bytes[..header_end]);
            let first=headers.lines().next().unwrap().to_owned();
            let length=headers.lines().find_map(|line|line.to_lowercase().strip_prefix("content-length:").and_then(|v|v.trim().parse::<usize>().ok())).unwrap_or(0);
            while bytes.len()<header_end+length{let n=stream.read(&mut buffer).await.unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);}
            let received:serde_json::Value=serde_json::from_slice(&bytes[header_end..header_end+length]).unwrap();
            let body=serde_json::to_string(&json!({"service":"Imdev loopback mock","request":first,"received":received,"notice":"Local test response; no external API called"})).unwrap();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Imdev-Mock: loopback-only\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
        });
        let request=imdev::core::Request{url:format!("http://{address}/v1/echo"),method:"POST".into(),query:vec![("preview".into(),"1".into())],headers:vec![("Accept".into(),"application/json".into()),("X-Demo".into(),"local-only".into())],body_type:"json".into(),body:"{\n  \"name\": \"Imdev\",\n  \"mode\": \"local-only\"\n}".into(),timeout_ms:2000};
        let(_tx,rx)=tokio::sync::watch::channel(false);
        let response=imdev::core::execute(request.clone(),rx).await.unwrap();
        server.await.unwrap();assert_eq!(response.status,200);
        (request,response)
    });
    for(name,width,height)in[("response-wide",1280.0,860.0),("response-compact",900.0,720.0)]{
        let ctx=egui::Context::default();let mut app=crate::ui::capture_app(&ctx);
        crate::ui::capture_response(&mut app,request.clone(),response.clone());
        dump(&ctx,&mut app,&Path::new(&base).join(name),width,height,&["Body","Pretty JSON"]);
    }
}

#[test]
fn export_row_alignment_preview_with_isolated_store() {
    let Ok(base)=std::env::var("IMDEV_ROW_CAPTURE_DIR") else{return;};
    let temporary=tempfile::tempdir().unwrap();
    for(name,width,height)in[("wide",1280.0,860.0),("compact",900.0,720.0)]{
        let ctx=egui::Context::default();
        let path=temporary.path().join(format!("{name}-state.json"));
        let mut app=crate::ui::capture_app_with_store(&ctx,path.clone());
        let folder=Path::new(&base).join(name);
        dump(&ctx,&mut app,&folder,width,height,&[]);
        let mut geometry=Vec::new();
        for label in ["Clear history","Timeout input"]{
            let(visible,hit)=ctx.data_mut(|d|d.get_temp::<(egui::Rect,egui::Rect)>(egui::Id::new(("compact_row_geometry",label))).unwrap());
            geometry.push(json!({"label":label,"visible":[visible.min.x,visible.min.y,visible.max.x,visible.max.y],"hit":[hit.min.x,hit.min.y,hit.max.x,hit.max.y]}));
        }
        fs::write(folder.join("row-geometry.json"),serde_json::to_vec_pretty(&geometry).unwrap()).unwrap();
        assert!(!path.exists(),"Preview must not write history or templates");
    }
}

#[test]
fn export_actual_auth_restore_stages() {
    let Ok(base)=std::env::var("IMDEV_AUTH_RESTORE_CAPTURE_DIR") else{return;};
    let temporary=tempfile::tempdir().unwrap();
    let ctx=egui::Context::default();let mut app=crate::ui::capture_app_with_store(&ctx,temporary.path().join("settings.json"));
    crate::ui::capture_auth_settings(&mut app);
    dump(&ctx,&mut app,&Path::new(&base).join("stage-01-auth-settings-wide"),1280.0,860.0,&[]);
    for(name,width,height,sensitive)in[("stage-02-request-restored-wide",1280.0,860.0,false),("stage-02-request-restored-compact",900.0,720.0,false),("stage-03-sensitive-reentry",1280.0,860.0,true)]{
        let ctx=egui::Context::default();let mut app=crate::ui::capture_app_with_store(&ctx,temporary.path().join(format!("{name}.json")));
        crate::ui::capture_restored(&mut app,sensitive);
        dump(&ctx,&mut app,&Path::new(&base).join(name),width,height,&[]);
    }
}

#[test]
fn export_actual_pair_table_stages() {
    let Ok(base)=std::env::var("IMDEV_PAIR_TABLE_CAPTURE_DIR") else{return;};
    let temporary=tempfile::tempdir().unwrap();
    for(name,width,height,headers)in[("stage-01-query-table-wide",1280.0,860.0,false),("stage-02-headers-table-wide",1280.0,860.0,true),("stage-01-query-table-compact",760.0,860.0,false)] {
        let ctx=egui::Context::default();let mut app=crate::ui::capture_app_with_store(&ctx,temporary.path().join(format!("{name}.json")));
        crate::ui::capture_pair_table(&mut app,headers);
        dump(&ctx,&mut app,&Path::new(&base).join(name),width,height,&[]);
    }
}

#[test]
fn export_actual_native_preview_frames() {
    let Ok(base) = std::env::var("IMDEV_CAPTURE_DIR") else { return; };
    let base=Path::new(&base);
    for (name,width,height) in [("empty-wide",1280.0,860.0),("empty-compact",900.0,720.0)] {
        let ctx=egui::Context::default();
        let mut app=crate::ui::capture_app(&ctx);
        dump(&ctx,&mut app,&base.join(name),width,height,&[]);
    }
}
