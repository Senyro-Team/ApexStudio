//! Isolated import benchmark. Never opens the active workspace or modifies input.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use apex_storage::Workspace;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Instant,
};
fn main() {
    let source = PathBuf::from(std::env::args().nth(1).expect("isolated source folder"));
    let temp = tempfile::tempdir().unwrap();
    let ws = Workspace::open(temp.path().join("workspace")).unwrap();
    let project = ws
        .save_project(
            None,
            apex_core::ProjectInput {
                name: "Benchmark".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let vehicle = apex_core::vehicle_catalogue()
        .unwrap()
        .vehicles
        .remove(0)
        .id;
    let all = Instant::now();
    let mut manifest = Vec::new();
    let mut paths = Vec::new();
    for folder in std::fs::read_dir(source).unwrap() {
        let folder = folder.unwrap().path();
        if !folder.is_dir() {
            continue;
        }
        let mut indexes = Vec::new();
        for file in std::fs::read_dir(&folder).unwrap() {
            let path = file.unwrap().path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            {
                apex_storage::png_dimensions(&path).unwrap();
                indexes.push(paths.len());
                paths.push(path);
            }
        }
        if !indexes.is_empty() {
            manifest.push((
                folder.file_name().unwrap().to_string_lossy().to_string(),
                indexes,
            ));
        }
    }
    let manifest_seconds = all.elapsed().as_secs_f64();
    let started = Instant::now();
    let cursor = AtomicUsize::new(0);
    let outputs = Mutex::new(vec![None; paths.len()]);
    let before = ws.validation_count();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| loop {
                let i = cursor.fetch_add(1, Ordering::Relaxed);
                let Some(path) = paths.get(i) else {
                    break;
                };
                let prepared = ws.prepare_texture(path).unwrap();
                outputs.lock().unwrap()[i] = Some(prepared);
            });
        }
    });
    let preparation_seconds = started.elapsed().as_secs_f64();
    let prepared = outputs.into_inner().unwrap();
    let decoded = ws.validation_count() - before;
    let commit = Instant::now();
    for (name, indices) in &manifest {
        let files = indices
            .iter()
            .map(|i| {
                (
                    prepared[*i].as_ref().unwrap().clone(),
                    paths[*i].file_name().unwrap().to_string_lossy().to_string(),
                    None,
                )
            })
            .collect::<Vec<_>>();
        ws.import_prepared_livery(&project.id, None, None, name, &vehicle, &files, None)
            .unwrap();
    }
    let commit_seconds = commit.elapsed().as_secs_f64();
    let usable_seconds = all.elapsed().as_secs_f64();
    assert_eq!(
        ws.validation_count() - before,
        decoded,
        "commit must not decode again"
    );
    let thumbs = Instant::now();
    for p in prepared.iter().flatten() {
        ws.thumbnail_bytes(p.hash()).unwrap();
    }
    println!(
        "{}",
        serde_json::json!({"images":paths.len(),"liveries":manifest.len(),"manifestSeconds":manifest_seconds,"prepareSeconds":preparation_seconds,"commitSeconds":commit_seconds,"usableSeconds":usable_seconds,"thumbnailTailSeconds":thumbs.elapsed().as_secs_f64(),"totalSeconds":all.elapsed().as_secs_f64(),"uniqueDecodes":decoded,"peakWorkers":ws.pipeline_peak().0,"peakDecodedBudgetBytes":ws.pipeline_peak().1})
    );
}
