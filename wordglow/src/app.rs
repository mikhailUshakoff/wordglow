use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine};
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use crate::bindings::{invoke, invoke0};
use crate::dto::{
    BookDto, DictionaryEntryDto, ImportSummaryDto, LessonAudioDto, LessonDetailDto, LessonDto, LessonStatus,
    QuestionDto,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BookIdArgs {
    book_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LessonIdArgs {
    lesson_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranslateArgs {
    word: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveWordArgs {
    word: String,
    lesson_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoveWordArgs {
    id: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetLessonStatusArgs {
    lesson_id: String,
    status: LessonStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportPackageArgs {
    data_base64: String,
}

/// Shared reactive state for the whole app, provided as a Leptos context so
/// nested views (book list, lesson list, reader) don't need long prop lists.
#[derive(Clone, Copy)]
struct AppState {
    books: RwSignal<Vec<BookDto>>,
    selected_book: RwSignal<Option<BookDto>>,
    lessons: RwSignal<Vec<LessonDto>>,
    selected_lesson: RwSignal<Option<LessonDto>>,
    statuses: RwSignal<HashMap<String, LessonStatus>>,
    error: RwSignal<Option<String>>,
    showing_vocabulary: RwSignal<bool>,
}

fn state() -> AppState {
    use_context::<AppState>().expect("AppState provided by <App>")
}

#[component]
pub fn App() -> impl IntoView {
    let state = AppState {
        books: RwSignal::new(Vec::new()),
        selected_book: RwSignal::new(None),
        lessons: RwSignal::new(Vec::new()),
        selected_lesson: RwSignal::new(None),
        statuses: RwSignal::new(HashMap::new()),
        error: RwSignal::new(None),
        showing_vocabulary: RwSignal::new(false),
    };
    provide_context(state);

    spawn_local(async move {
        match invoke0::<Vec<BookDto>>("list_books").await {
            Ok(list) => state.books.set(list),
            Err(e) => state.error.set(Some(e)),
        }
    });

    spawn_local(async move {
        match invoke0::<HashMap<String, LessonStatus>>("get_lesson_statuses").await {
            Ok(statuses) => state.statuses.set(statuses),
            Err(e) => state.error.set(Some(e)),
        }
    });

    view! {
        <div class="app">
            <h1>"Wordglow Library"</h1>
            {move || {
                state.error.get().map(|message| view! { <p class="error">{message}</p> })
            }}
            <Show when=move || !state.showing_vocabulary.get()>
                <button class="nav-link" on:click=move |_| state.showing_vocabulary.set(true)>
                    "Vocabulary"
                </button>
            </Show>
            <ImportButton />
            <Show when=move || state.showing_vocabulary.get() fallback=|| view! { <Library /> }>
                <VocabularyView />
            </Show>
        </div>
    }
}

/// Import a content package (a .zip produced by `teacher-cli export`).
/// Reads the file client-side and ships it to the backend as base64, the
/// same pattern already used to carry lesson audio the other direction.
#[component]
fn ImportButton() -> impl IntoView {
    let state = state();
    let status = RwSignal::new(None::<String>);
    let input_ref = NodeRef::<html::Input>::new();

    let on_change = move |_| {
        let Some(input) = input_ref.get() else { return };
        let Some(files) = input.files() else { return };
        let Some(file) = files.get(0) else { return };

        let reader = web_sys::FileReader::new().expect("FileReader::new");
        let reader_for_load = reader.clone();
        let onload = Closure::once(move || {
            let Ok(result) = reader_for_load.result() else { return };
            let Ok(array_buffer) = result.dyn_into::<js_sys::ArrayBuffer>() else { return };
            let bytes = js_sys::Uint8Array::new(&array_buffer).to_vec();
            let data_base64 = STANDARD.encode(bytes);

            spawn_local(async move {
                match invoke::<ImportSummaryDto>("import_package", ImportPackageArgs { data_base64 }).await {
                    Ok(summary) => {
                        status.set(Some(format!(
                            "Imported {} book(s), {} lesson(s), {} audio file(s), {} question(s)",
                            summary.books, summary.lessons, summary.audio, summary.questions
                        )));
                        spawn_local(async move {
                            match invoke0::<Vec<BookDto>>("list_books").await {
                                Ok(list) => state.books.set(list),
                                Err(e) => state.error.set(Some(e)),
                            }
                        });
                        spawn_local(async move {
                            match invoke0::<HashMap<String, LessonStatus>>("get_lesson_statuses").await {
                                Ok(statuses) => state.statuses.set(statuses),
                                Err(e) => state.error.set(Some(e)),
                            }
                        });
                    }
                    Err(e) => state.error.set(Some(e)),
                }
            });
        });
        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
        onload.forget();
        let _ = reader.read_as_array_buffer(&file);
    };

    view! {
        <span class="import-package">
            <label class="nav-link import-label">
                "Import package"
                <input
                    node_ref=input_ref
                    type="file"
                    accept=".zip"
                    style="display:none"
                    on:change=on_change
                />
            </label>
            {move || status.get().map(|s| view! { <span class="import-status">{s}</span> })}
        </span>
    }
}

#[component]
fn Library() -> impl IntoView {
    let state = state();

    view! {
        <Show
            when=move || state.selected_lesson.get().is_none()
            fallback=|| view! { <ReadingView /> }
        >
            <Show
                when=move || state.selected_book.get().is_none()
                fallback=|| view! { <LessonList /> }
            >
                <BookList />
            </Show>
        </Show>
    }
}

#[component]
fn BookList() -> impl IntoView {
    let state = state();

    view! {
        <ul class="book-list">
            {move || {
                state
                    .books
                    .get()
                    .into_iter()
                    .map(|book| view! { <BookRow book=book /> })
                    .collect_view()
            }}
        </ul>
    }
}

#[component]
fn BookRow(book: BookDto) -> impl IntoView {
    let state = state();
    let title = book.title.clone();
    let author = book.author.clone().unwrap_or_default();
    let book_id = book.id.clone();

    let on_click = move |_| {
        state.selected_book.set(Some(book.clone()));
        let book_id = book_id.clone();
        spawn_local(async move {
            match invoke::<Vec<LessonDto>>("list_lessons", BookIdArgs { book_id: book_id.clone() }).await {
                Ok(list) => state.lessons.set(list),
                Err(e) => state.error.set(Some(e)),
            }
        });
    };

    view! {
        <li class="book-card" on:click=on_click>
            <span>{title}</span>
            <span>{author}</span>
        </li>
    }
}

#[component]
fn LessonList() -> impl IntoView {
    let state = state();
    let title = move || state.selected_book.get().map(|b| b.title).unwrap_or_default();

    view! {
        <div>
            <button class="back-link" on:click=move |_| state.selected_book.set(None)>
                "< Back to books"
            </button>
            <h2>{title}</h2>
            <ul class="lesson-list">
                {move || {
                    state
                        .lessons
                        .get()
                        .into_iter()
                        .map(|lesson| view! { <LessonRow lesson=lesson /> })
                        .collect_view()
                }}
            </ul>
        </div>
    }
}

#[component]
fn LessonRow(lesson: LessonDto) -> impl IntoView {
    let state = state();
    let label = format!("{}. {}", lesson.order_index + 1, lesson.title);
    let lesson_id = lesson.id.clone();
    let lesson_id_for_class = lesson.id.clone();

    let status = move |id: &str| state.statuses.get().get(id).copied().unwrap_or_default();

    view! {
        <li class="lesson-card" on:click=move |_| state.selected_lesson.set(Some(lesson.clone()))>
            <span>{label}</span>
            <span class=move || status(&lesson_id_for_class).css_class()>{move || status(&lesson_id).label()}</span>
        </li>
    }
}

#[component]
fn VocabularyView() -> impl IntoView {
    let state = state();
    let entries = RwSignal::new(Vec::<DictionaryEntryDto>::new());

    spawn_local(async move {
        match invoke0::<Vec<DictionaryEntryDto>>("list_dictionary").await {
            // Always start with translations hidden, even if a past visit (or
            // the reader's word menu) already cached one on the backend —
            // re-opening this tab should require a fresh "Translate" click.
            Ok(list) => entries.set(
                list.into_iter()
                    .map(|entry| DictionaryEntryDto { translation: None, ..entry })
                    .collect(),
            ),
            Err(e) => state.error.set(Some(e)),
        }
    });

    view! {
        <div>
            <button class="back-link" on:click=move |_| state.showing_vocabulary.set(false)>
                "< Back"
            </button>
            <h2>"Vocabulary"</h2>
            <Show when=move || entries.get().is_empty()>
                <p>"No words saved yet — click a word while reading to save it."</p>
            </Show>
            <ul class="dictionary-list">
                {move || {
                    entries
                        .get()
                        .into_iter()
                        .map(|entry| view! { <DictionaryRow entry=entry entries=entries /> })
                        .collect_view()
                }}
            </ul>
        </div>
    }
}

#[component]
fn DictionaryRow(entry: DictionaryEntryDto, entries: RwSignal<Vec<DictionaryEntryDto>>) -> impl IntoView {
    let state = state();
    let id_for_translate = entry.id.clone();
    let word_for_translate = entry.word.clone();
    let id_for_remove = entry.id.clone();

    let on_translate = move |_| {
        let id = id_for_translate.clone();
        let word = word_for_translate.clone();
        spawn_local(async move {
            match invoke::<String>("translate_word", TranslateArgs { word }).await {
                Ok(translation) => entries.update(|list| {
                    if let Some(e) = list.iter_mut().find(|e| e.id == id) {
                        e.translation = Some(translation);
                    }
                }),
                Err(e) => state.error.set(Some(e)),
            }
        });
    };

    let on_remove = move |_| {
        let id = id_for_remove.clone();
        spawn_local(async move {
            match invoke::<()>("remove_word", RemoveWordArgs { id: id.clone() }).await {
                Ok(()) => entries.update(|list| list.retain(|e| e.id != id)),
                Err(e) => state.error.set(Some(e)),
            }
        });
    };

    view! {
        <li class="dictionary-row">
            <span class="word">{entry.word.clone()}</span>
            {match entry.translation.clone() {
                Some(t) => view! { <span class="translation">{t}</span> }.into_any(),
                None => view! {
                    <button class="translate-btn" on:click=on_translate>
                        "Translate"
                    </button>
                }
                    .into_any(),
            }}
            <button class="remove-btn" on:click=on_remove>
                "Remove"
            </button>
        </li>
    }
}

#[component]
fn ReadingView() -> impl IntoView {
    let state = state();
    let lesson_id = state.selected_lesson.get_untracked().map(|l| l.id).unwrap_or_default();
    let lesson_id_for_save = lesson_id.clone();

    let detail = RwSignal::new(None::<LessonDetailDto>);
    let audio_info = RwSignal::new(None::<LessonAudioDto>);
    let questions = RwSignal::new(Vec::<QuestionDto>::new());
    let current_word = RwSignal::new(None::<usize>);
    let playback_rate = RwSignal::new(1.0_f64);
    let audio_ref = NodeRef::<html::Audio>::new();

    // Which word's action menu (if any) is open, plus its transient
    // translate/save results — reset whenever a different word is clicked.
    let word_menu = RwSignal::new(None::<usize>);
    let word_translation = RwSignal::new(None::<String>);
    let word_saved = RwSignal::new(false);

    {
        let lesson_id = lesson_id.clone();
        spawn_local(async move {
            match invoke::<LessonDetailDto>("get_lesson", LessonIdArgs { lesson_id: lesson_id.clone() }).await {
                Ok(d) => detail.set(Some(d)),
                Err(e) => state.error.set(Some(e)),
            }
        });
    }
    {
        let lesson_id = lesson_id.clone();
        spawn_local(async move {
            match invoke::<Option<LessonAudioDto>>("get_lesson_audio", LessonIdArgs { lesson_id: lesson_id.clone() })
                .await
            {
                Ok(a) => audio_info.set(a),
                Err(e) => state.error.set(Some(e)),
            }
        });
    }
    {
        let lesson_id = lesson_id.clone();
        spawn_local(async move {
            match invoke::<Vec<QuestionDto>>("get_lesson_questions", LessonIdArgs { lesson_id: lesson_id.clone() })
                .await
            {
                Ok(q) => questions.set(q),
                Err(e) => state.error.set(Some(e)),
            }
        });
    }

    // Opening a lesson counts as starting it — but don't downgrade a lesson
    // that's already in progress or completed.
    let was_not_started = !matches!(
        state.statuses.get_untracked().get(&lesson_id),
        Some(LessonStatus::InProgress) | Some(LessonStatus::Completed)
    );
    if was_not_started {
        state.statuses.update(|m| {
            m.insert(lesson_id.clone(), LessonStatus::InProgress);
        });
        let lesson_id = lesson_id.clone();
        spawn_local(async move {
            let _ = invoke::<()>(
                "set_lesson_status",
                SetLessonStatusArgs { lesson_id, status: LessonStatus::InProgress },
            )
            .await;
        });
    }

    // Word indices must stay in the same order `text.split_whitespace()` would
    // produce — that's what the TTS word_timepoints are keyed to. Splitting by
    // line first and then by whitespace within each line preserves that order
    // while letting the UI keep the source text's paragraph breaks.
    let paragraphs = move || {
        let mut index = 0usize;
        detail
            .get()
            .map(|d| {
                d.text
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(|line| {
                        line.split_whitespace()
                            .map(|word| {
                                let i = index;
                                index += 1;
                                (i, word.to_string())
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    let seek_to_word = move |index: usize| {
        if let Some(audio) = audio_info.get() {
            if let Some(tp) = audio.word_timepoints.get(index) {
                if let Some(el) = audio_ref.get() {
                    el.set_current_time(tp.start_ms as f64 / 1000.0);
                    let _ = el.play();
                }
            }
        }
    };

    // `timeupdate` only fires a handful of times a second in the webview, which
    // made the highlight visibly skip words. Drive it from `requestAnimationFrame`
    // instead so it tracks `currentTime` every frame the audio is playing.
    Effect::new(move |_| {
        let Some(el) = audio_ref.get() else { return };
        let alive = Arc::new(AtomicBool::new(true));
        let alive_for_closure = alive.clone();
        let cell: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let cell_for_closure = cell.clone();
        *cell.borrow_mut() = Some(Closure::new(move || {
            if !alive_for_closure.load(Ordering::Relaxed) {
                cell_for_closure.borrow_mut().take();
                return;
            }
            if !el.paused() {
                let ms = (el.current_time() * 1000.0) as u64;
                if let Some(audio) = audio_info.get_untracked() {
                    let active = audio
                        .word_timepoints
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, tp)| tp.start_ms <= ms)
                        .map(|(i, _)| i);
                    if current_word.get_untracked() != active {
                        current_word.set(active);
                    }
                }
            }
            if let (Some(win), Some(cb)) = (web_sys::window(), cell_for_closure.borrow().as_ref()) {
                let _ = win.request_animation_frame(cb.as_ref().unchecked_ref());
            }
        }));
        if let (Some(win), Some(cb)) = (web_sys::window(), cell.borrow().as_ref()) {
            let _ = win.request_animation_frame(cb.as_ref().unchecked_ref());
        }
        on_cleanup(move || alive.store(false, Ordering::Relaxed));
    });

    let on_ended = move |_| {
        let lesson_id = state.selected_lesson.get_untracked().map(|l| l.id).unwrap_or_default();
        state.statuses.update(|m| {
            m.insert(lesson_id.clone(), LessonStatus::Completed);
        });
        spawn_local(async move {
            let _ = invoke::<()>(
                "set_lesson_status",
                SetLessonStatusArgs { lesson_id, status: LessonStatus::Completed },
            )
            .await;
        });
    };

    let on_rate_input = move |ev| {
        let value: f64 = event_target_value(&ev).parse().unwrap_or(1.0);
        playback_rate.set(value);
        if let Some(el) = audio_ref.get() {
            el.set_playback_rate(value);
        }
    };

    view! {
        <div class="reader">
            <button
                class="back-link"
                on:click=move |_| {
                    state.selected_lesson.set(None);
                    current_word.set(None);
                }
            >
                "< Back to lessons"
            </button>
            <h2>{move || detail.get().map(|d| d.title).unwrap_or_default()}</h2>

            {move || {
                audio_info
                    .get()
                    .filter(|a| a.stale)
                    .map(|_| view! { <p class="error">"Audio is stale — regenerate it with teacher-cli."</p> })
            }}
            {move || {
                if detail.get().is_some() && audio_info.get().is_none() {
                    Some(view! { <p class="error">"No audio yet for this lesson — run generate-tts in teacher-cli."</p> })
                } else {
                    None
                }
            }}

            {move || {
                audio_info.get().map(|audio| {
                    let src = format!("data:{};base64,{}", audio.mime, audio.audio_base64);
                    view! {
                        <div class="player">
                            <audio node_ref=audio_ref src=src preload="auto" on:ended=on_ended></audio>
                            <div class="controls">
                                <button on:click=move |_| { if let Some(el) = audio_ref.get() { let _ = el.play(); } }>
                                    "Play"
                                </button>
                                <button on:click=move |_| { if let Some(el) = audio_ref.get() { let _ = el.pause(); } }>
                                    "Pause"
                                </button>
                                <button on:click=move |_| {
                                    if let Some(el) = audio_ref.get() {
                                        let _ = el.pause();
                                        el.set_current_time(0.0);
                                    }
                                }>
                                    "Stop"
                                </button>
                                <label class="speed">
                                    "Speed "
                                    <input
                                        type="range"
                                        min="0.5"
                                        max="2"
                                        step="0.05"
                                        prop:value=move || playback_rate.get().to_string()
                                        on:input=on_rate_input
                                    />
                                    {move || format!("{:.2}x", playback_rate.get())}
                                </label>
                            </div>
                        </div>
                    }
                })
            }}

            <div class="lesson-text">
                {move || {
                    paragraphs()
                        .into_iter()
                        .map(|paragraph| {
                            view! {
                                <p>
                                    {paragraph
                                        .into_iter()
                                        .map(|(i, word)| {
                                            let active = move || current_word.get() == Some(i);
                            let word_for_menu = word.clone();
                            let word_for_translate = word.clone();
                            let word_for_save = word.clone();
                            let lesson_id_for_save = lesson_id_for_save.clone();
                            view! {
                                <span class="word-wrap">
                                    <span
                                        class:word-active=active
                                        class="word"
                                        on:click=move |_| {
                                            if let Some(el) = audio_ref.get() {
                                                let _ = el.pause();
                                            }
                                            word_menu.set(Some(i));
                                            word_translation.set(None);
                                            word_saved.set(false);
                                        }
                                    >
                                        {word}
                                        " "
                                    </span>
                                    {move || {
                                        let word_for_menu = word_for_menu.clone();
                                        let word_for_translate = word_for_translate.clone();
                                        let word_for_save = word_for_save.clone();
                                        let lesson_id_for_save = lesson_id_for_save.clone();
                                        (word_menu.get() == Some(i)).then(move || {
                                            view! {
                                                <div class="word-menu">
                                                    <button
                                                        class="word-menu-close"
                                                        on:click=move |_| {
                                                            word_menu.set(None);
                                                            word_translation.set(None);
                                                            word_saved.set(false);
                                                        }
                                                    >
                                                        "\u{d7}"
                                                    </button>
                                                    <p class="word-menu-word">{word_for_menu}</p>
                                                    <div class="word-menu-actions">
                                                        <button on:click=move |_| {
                                                            seek_to_word(i);
                                                            word_menu.set(None);
                                                            word_translation.set(None);
                                                            word_saved.set(false);
                                                        }>
                                                            "Continue from here"
                                                        </button>
                                                        <button on:click=move |_| {
                                                            let word = word_for_translate.clone();
                                                            spawn_local(async move {
                                                                match invoke::<String>("translate_word", TranslateArgs { word }).await {
                                                                    Ok(t) => word_translation.set(Some(t)),
                                                                    Err(e) => state.error.set(Some(e)),
                                                                }
                                                            });
                                                        }>
                                                            "Translate"
                                                        </button>
                                                        <button on:click=move |_| {
                                                            let word = word_for_save.clone();
                                                            let lesson_id = lesson_id_for_save.clone();
                                                            spawn_local(async move {
                                                                match invoke::<DictionaryEntryDto>(
                                                                    "save_word",
                                                                    SaveWordArgs { word, lesson_id: Some(lesson_id) },
                                                                )
                                                                    .await
                                                                {
                                                                    Ok(_) => word_saved.set(true),
                                                                    Err(e) => state.error.set(Some(e)),
                                                                }
                                                            });
                                                        }>
                                                            "Save word"
                                                        </button>
                                                    </div>
                                                    {move || {
                                                        word_translation
                                                            .get()
                                                            .map(|t| view! { <p class="word-menu-translation">{t}</p> })
                                                    }}
                                                    {move || {
                                                        word_saved
                                                            .get()
                                                            .then(|| view! { <p class="word-menu-saved">"Saved to vocabulary"</p> })
                                                    }}
                                                </div>
                                            }
                                        })
                                    }}
                                </span>
                            }
                        })
                        .collect_view()}
                                </p>
                            }
                        })
                        .collect_view()
                }}
            </div>

            <Show when=move || !questions.get().is_empty()>
                <div class="lesson-questions">
                    <h3>"Comprehension questions"</h3>
                    <ol>
                        {move || {
                            questions
                                .get()
                                .into_iter()
                                .map(|q| view! { <li>{q.question_text}</li> })
                                .collect_view()
                        }}
                    </ol>
                </div>
            </Show>
        </div>
    }
}
