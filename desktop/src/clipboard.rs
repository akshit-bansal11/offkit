//! The system clipboard. One instance lives on the UI thread for the app's lifetime: on Linux
//! the copied text is served by that instance, so dropping it after each copy would lose it.

use std::cell::RefCell;

thread_local! {
    static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
}

/// Puts `text` on the clipboard. The error reads as a sentence for the tool to show.
pub fn copy_text(text: &str) -> anyhow::Result<()> {
    CLIPBOARD.with_borrow_mut(|slot| {
        let clipboard = match slot {
            Some(clipboard) => clipboard,
            None => slot.insert(arboard::Clipboard::new()?),
        };
        clipboard.set_text(text)?;
        Ok(())
    })
}
