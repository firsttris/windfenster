//! The log's warnings and errors also go to the status board, for the admin page's log.

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

pub struct ToBoard;

impl<S: Subscriber> Layer<S> for ToBoard {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let level = *event.metadata().level();
        if level > Level::WARN {
            return;
        }
        let mut text = Text(String::new());
        event.record(&mut text);
        wf_core::status::board().log(level.as_str(), event.metadata().target(), text.0);
    }
}

/// The message and the other fields of an event, as the log prints them
struct Text(String);

impl Visit for Text {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if field.name() == "message" {
            self.0.push_str(&format!("{value:?}"));
        } else {
            self.0.push_str(&format!("{}={value:?}", field.name()));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(value);
        } else {
            self.record_debug(field, &value);
        }
    }
}
