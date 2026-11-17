//! Speakers after processing (screens/5, screens/6): what the Name speakers
//! dialog shows, renames kept, and the audio a line or a voice plays.

mod recorded;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use recap::{
    naming::NAMES,
    playback::{self, Clip, Output, Player, PlayerEvent},
    speakers,
};
use recap_engine::{Recording, SAMPLE_RATE, Source};
use recap_storage::{MeetingId, Segment, Speaker, Store};
use serde_json::json;

fn segment(source: Source, speaker: &str, start_ms: u64, end_ms: u64, text: &str) -> Segment {
    Segment { source, speaker: Some(speaker.into()), start_ms, end_ms, text: text.into() }
}

/// The test meeting, refined: Me, then three voices; S3 unnamed.
fn meeting(store: &Store) -> MeetingId {
    let id = recorded::stopped(store);
    store
        .set_refined(
            &id,
            &[
                segment(Source::Me, "me", 0, 9_000, "Okay, let's get started. Thanks for joining, Priya and David. Today I want to settle the launch date for the mobile app and agree on who owns the remaining work."),
                segment(Source::Them, "S1", 9_000, 21_000, "Thanks, Tom. Quick status from design. The onboarding screens are done."),
                segment(Source::Them, "S2", 21_000, 33_000, "From the backend side, the payments API is ready, but load testing showed slow responses above two thousand users."),
                segment(Source::Me, "me", 33_000, 40_000, "One more week pushes us to December first."),
                segment(Source::Them, "S1", 40_000, 45_000, "Yes, I can finish dark mode by Friday."),
                segment(Source::Them, "S2", 45_000, 47_000, "Sure."),
                segment(Source::Them, "S2", 47_000, 56_000, "I'll also send the load test report to the whole team by Thursday."),
            ],
        )
        .unwrap();
    store
        .update(&id, |info| {
            info.speakers = vec![
                Speaker { id: "me".into(), name: Some("Vu".into()) },
                Speaker { id: "S1".into(), name: Some("Priya".into()) },
                Speaker { id: "S2".into(), name: None },
            ];
        })
        .unwrap();
    id
}

#[test]
fn the_dialog_shows_each_speaker_with_two_quotes_and_a_suggestion() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = meeting(&store);
    // The AI's guesses: S2's was not sure enough to be taken, but is a hint.
    std::fs::write(
        store.folder(&id).join(NAMES),
        json!({
            "S1": { "name": "Priya", "evidence": "Priya, can design finish", "confidence": "high" },
            "S2": { "name": "David", "evidence": "Thanks for joining, Priya and David.", "confidence": "low" }
        })
        .to_string(),
    )
    .unwrap();
    let view = speakers::dialog(&store, &id).unwrap();
    assert_eq!(view["id"], id.as_str());
    let list = view["speakers"].as_array().unwrap();
    assert_eq!(list.len(), 3);

    // Chips say who the voice is to the engine (screens/6); the field its name.
    // Me: my name, locked (Settings gives it).
    assert_eq!(list[0]["id"], "me");
    assert_eq!(list[0]["label"], "Me");
    assert_eq!(list[0]["name"], "Vu");
    assert_eq!(list[0]["n"], 1);
    assert_eq!(list[0]["locked"], true);
    assert_eq!(
        list[0]["quotes"],
        json!([
            "Okay, let's get started. Thanks for joining, Priya and David. Today I…",
            "One more week pushes us to December first."
        ])
    );

    assert_eq!(list[1]["label"], "Speaker 1");
    assert_eq!(list[1]["name"], "Priya");
    assert_eq!(list[1]["locked"], false);
    assert_eq!(list[1]["suggest"], "", "named already: nothing to suggest");

    // Unnamed: "Speaker 2", no name, the AI's hint, quotes long enough to tell.
    assert_eq!(list[2]["label"], "Speaker 2");
    assert_eq!(list[2]["name"], "");
    assert_eq!(list[2]["suggest"], "David");
    assert_eq!(
        list[2]["quotes"],
        json!([
            "From the backend side, the payments API is ready, but load testing…",
            "I'll also send the load test report to the whole team by Thursday."
        ])
    );
    // Known people to pick from.
    assert_eq!(view["people"], json!(["Priya", "Vu"]));
}

#[test]
fn renames_are_kept_and_me_stays_my_name() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = meeting(&store);
    let names = vec![
        ("S2".to_string(), Some("David".to_string())),
        ("S1".to_string(), Some("Priya".to_string())),
        ("me".to_string(), Some("Tom".to_string())),
    ];
    let changed = speakers::rename(&store, &id, &names).unwrap();
    assert_eq!(changed, ["S2"], "only what changed; Me is locked");
    let info = store.meeting(&id).unwrap().unwrap().info;
    let got: Vec<_> = info.speakers.iter().map(|s| (s.id.as_str(), s.name.as_deref())).collect();
    assert_eq!(got, [("me", Some("Vu")), ("S1", Some("Priya")), ("S2", Some("David"))]);
    assert!(store.people().unwrap().contains(&"David".to_string()));

    // Unnaming goes back to "Speaker N".
    let changed = speakers::rename(&store, &id, &[("S1".to_string(), None)]).unwrap();
    assert_eq!(changed, ["S1"]);
    assert_eq!(store.meeting(&id).unwrap().unwrap().info.speakers[1].name, None);
    assert!(speakers::rename(&store, &id, &[("S7".to_string(), Some("Lan".to_string()))]).is_err());
}

/// One side of a recording: `secs` seconds where second `i` holds the value `i + 1`.
fn counting(secs: usize) -> Vec<i16> {
    (0..secs * SAMPLE_RATE as usize).map(|i| (i / SAMPLE_RATE as usize + 1) as i16).collect()
}

fn at_ms(samples: &[i16], ms: u64) -> i16 {
    samples[(ms * u64::from(SAMPLE_RATE) / 1000) as usize]
}

#[test]
fn a_line_plays_both_sides_from_its_start_to_the_end() {
    let rec = Recording { me: counting(10), them: counting(10).iter().map(|s| s * 100).collect() };
    let clip = playback::from_line(&rec, 4_000);
    assert_eq!(clip.samples.len(), 6 * SAMPLE_RATE as usize);
    // Me and Them mixed: second 5 is 5 + 500.
    assert_eq!(at_ms(&clip.samples, 0), 505);
    assert_eq!(at_ms(&clip.samples, 5_999), 1010);
    assert_eq!(clip.duration_ms(), 6_000);
    // Past the end: nothing to play.
    assert!(playback::from_line(&rec, 12_000).samples.is_empty());
}

#[test]
fn a_speaker_clip_is_their_longest_turns_on_their_side() {
    let rec = Recording { me: counting(60), them: counting(60).iter().map(|s| s * 100).collect() };
    let segments = vec![
        segment(Source::Them, "S1", 1_000, 3_000, "short"),
        segment(Source::Them, "S2", 5_000, 6_000, "other voice"),
        segment(Source::Them, "S1", 10_000, 15_000, "longest"),
        segment(Source::Me, "me", 16_000, 30_000, "mine"),
        segment(Source::Them, "S1", 40_000, 43_000, "second longest"),
    ];
    let clip = playback::speaker_clip(&rec, &segments, "S1");
    // The longest turns up to 8 s, in the order said, a short gap between:
    // 10-15 s, then 40-43 s.
    let gap = playback::GAP_MS;
    assert_eq!(clip.duration_ms(), 5_000 + gap + 3_000);
    assert_eq!(at_ms(&clip.samples, 0), 1100, "Them's channel, second 11");
    assert_eq!(at_ms(&clip.samples, 4_999), 1500);
    assert_eq!(at_ms(&clip.samples, 5_000 + gap / 2), 0, "silence between turns");
    assert_eq!(at_ms(&clip.samples, 5_000 + gap), 4100);

    // Me plays from the mic side; a long turn is cut at 8 s.
    let mine = playback::speaker_clip(&rec, &segments, "me");
    assert_eq!(mine.duration_ms(), playback::CLIP_MS);
    assert_eq!(at_ms(&mine.samples, 0), 17);
    // A speaker who never spoke: nothing.
    assert!(playback::speaker_clip(&rec, &segments, "S9").samples.is_empty());
}

/// An output that plays for as long as it was told to (no sound card).
#[derive(Clone, Default)]
struct FakeOutput {
    played: Arc<Mutex<Vec<usize>>>,
    playing: Arc<Mutex<bool>>,
}

impl Output for FakeOutput {
    fn play(&mut self, clip: Clip) -> Result<(), String> {
        self.played.lock().unwrap().push(clip.samples.len());
        *self.playing.lock().unwrap() = true;
        Ok(())
    }
    fn playing(&self) -> bool {
        *self.playing.lock().unwrap()
    }
    fn stop(&mut self) {
        *self.playing.lock().unwrap() = false;
    }
}

async fn next(events: &mut tokio::sync::mpsc::UnboundedReceiver<PlayerEvent>) -> PlayerEvent {
    tokio::time::timeout(Duration::from_secs(20), events.recv()).await.expect("the player answers").expect("open")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_player_plays_on_its_own_thread_and_says_when_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = meeting(&store);
    let audio = store.folder(&id).join("audio.ogg");
    let output = FakeOutput::default();
    let (player, mut events) = Player::spawn({
        let output = output.clone();
        move || Box::new(output) as Box<dyn Output>
    });

    player.play("line-3", audio.clone(), playback::What::Line { from_ms: 21_000 });
    let PlayerEvent::Playing { key, duration_ms } = next(&mut events).await else { panic!("playing") };
    assert_eq!(key, "line-3");
    // The test meeting runs about 118 s.
    assert!((90_000..100_000).contains(&duration_ms), "{duration_ms}");

    // Another one stops the first.
    player.play(
        "speaker-S2",
        audio.clone(),
        playback::What::Speaker { source: Source::Them, turns: vec![(21_000, 26_000)] },
    );
    assert_eq!(next(&mut events).await, PlayerEvent::Ended { key: "line-3".into() });
    assert_eq!(next(&mut events).await, PlayerEvent::Playing { key: "speaker-S2".into(), duration_ms: 5_000 });
    assert_eq!(output.played.lock().unwrap().len(), 2);

    // It ends on its own.
    *output.playing.lock().unwrap() = false;
    assert_eq!(next(&mut events).await, PlayerEvent::Ended { key: "speaker-S2".into() });

    // Stop; and a recording that isn't there says so.
    player.play("line-1", audio.clone(), playback::What::Line { from_ms: 0 });
    assert!(matches!(next(&mut events).await, PlayerEvent::Playing { .. }));
    player.stop();
    assert_eq!(next(&mut events).await, PlayerEvent::Ended { key: "line-1".into() });
    player.play("line-2", dir.path().join("nope.ogg"), playback::What::Line { from_ms: 0 });
    let PlayerEvent::Failed { key, error } = next(&mut events).await else { panic!("failed") };
    assert_eq!(key, "line-2");
    assert_eq!(error, "This meeting's recording is gone.");
}
