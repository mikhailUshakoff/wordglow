use std::collections::HashMap;

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Serialize;

use crate::bindings::{invoke, invoke0};
use crate::dto::{BookDto, LessonDto, LessonStatus};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListLessonsArgs {
    book_id: String,
}

#[component]
pub fn App() -> impl IntoView {
    let books = RwSignal::new(Vec::<BookDto>::new());
    let selected = RwSignal::new(None::<BookDto>);
    let lessons = RwSignal::new(Vec::<LessonDto>::new());
    let statuses = RwSignal::new(HashMap::<String, LessonStatus>::new());
    let error = RwSignal::new(None::<String>);

    spawn_local(async move {
        match invoke0::<Vec<BookDto>>("list_books").await {
            Ok(list) => books.set(list),
            Err(e) => error.set(Some(e)),
        }
    });

    view! {
        <div class="app">
            <h1>"Wordglow Library"</h1>
            {move || {
                error.get().map(|message| view! { <p class="error">{message}</p> })
            }}
            <Show
                when=move || selected.get().is_none()
                fallback=move || view! { <LessonView selected=selected lessons=lessons statuses=statuses /> }
            >
                <BookList books=books selected=selected lessons=lessons error=error />
            </Show>
        </div>
    }
}

#[component]
fn BookList(
    books: RwSignal<Vec<BookDto>>,
    selected: RwSignal<Option<BookDto>>,
    lessons: RwSignal<Vec<LessonDto>>,
    error: RwSignal<Option<String>>,
) -> impl IntoView {
    view! {
        <ul class="book-list">
            {move || {
                books
                    .get()
                    .into_iter()
                    .map(|book| view! { <BookRow book=book selected=selected lessons=lessons error=error /> })
                    .collect_view()
            }}
        </ul>
    }
}

#[component]
fn BookRow(
    book: BookDto,
    selected: RwSignal<Option<BookDto>>,
    lessons: RwSignal<Vec<LessonDto>>,
    error: RwSignal<Option<String>>,
) -> impl IntoView {
    let title = book.title.clone();
    let author = book.author.clone().unwrap_or_default();
    let book_id = book.id.clone();

    let on_click = move |_| {
        selected.set(Some(book.clone()));
        let book_id = book_id.clone();
        spawn_local(async move {
            match invoke::<Vec<LessonDto>>("list_lessons", ListLessonsArgs { book_id }).await {
                Ok(list) => lessons.set(list),
                Err(e) => error.set(Some(e)),
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
fn LessonView(
    selected: RwSignal<Option<BookDto>>,
    lessons: RwSignal<Vec<LessonDto>>,
    statuses: RwSignal<HashMap<String, LessonStatus>>,
) -> impl IntoView {
    let title = move || selected.get().map(|b| b.title).unwrap_or_default();

    view! {
        <div>
            <button class="back-link" on:click=move |_| selected.set(None)>
                "< Back to books"
            </button>
            <h2>{title}</h2>
            <ul class="lesson-list">
                {move || {
                    lessons
                        .get()
                        .into_iter()
                        .map(|lesson| {
                            let status = statuses.get().get(&lesson.id).copied().unwrap_or_default();
                            view! {
                                <li class="lesson-card">
                                    <span>{format!("{}. {}", lesson.order_index + 1, lesson.title)}</span>
                                    <span class=status.css_class()>{status.label()}</span>
                                </li>
                            }
                        })
                        .collect_view()
                }}
            </ul>
        </div>
    }
}
