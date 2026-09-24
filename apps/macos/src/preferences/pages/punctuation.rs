//! 「通用」页「标点映射」的子弹窗：逐行编辑「原符号 → 转换文本」，保存进 `[general] punctuation_map`。
//! 草稿就放在行控件里（同自定义短语的做法），加行 / 删行后整表重建、行号 tag 重新分配。

use std::cell::RefCell;
use std::collections::BTreeMap;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBackingStoreType, NSBorderType, NSColor, NSScrollView, NSTextAlignment, NSTextField, NSView,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use qingjian_core::punctuation::{MAX_MAP_TEXT_CHARS, is_valid_map_key};

use crate::preferences::controls::{NOTE_HEIGHT, button, note_full, small_label, text_field};
use crate::preferences::layout::{Layout, PAGE_PADDING, ROW_HEIGHT};
use crate::preferences::setting::{MAX_PUNCTUATION_ROWS, Setting};
use crate::preferences::target::PreferencesTarget;

/// 编辑器宽度。
const EDITOR_WIDTH: f64 = 480.0;

/// 一行的高度（含行距）。
const ROW: f64 = ROW_HEIGHT + 4.0;

/// 列表区高度：固定十行，超出滚动。
const LIST_HEIGHT: f64 = 10.0 * ROW;

/// 原符号框的宽度。
const KEY_WIDTH: f64 = 44.0;

/// 箭头列的 x 与宽度。
const ARROW_X: f64 = KEY_WIDTH + 4.0;
const ARROW_WIDTH: f64 = 18.0;

/// 转换文本框的 x。
const VALUE_X: f64 = ARROW_X + ARROW_WIDTH + 6.0;

/// 「删」按钮的宽度。
const REMOVE_WIDTH: f64 = 48.0;

/// 一行映射的两个文本框（删按钮 wire 完交给视图树持有）。
struct Row {
    key: Retained<NSTextField>,
    value: Retained<NSTextField>,
}

pub(super) struct PunctuationEditor {
    /// 子弹窗本体。
    window: Retained<NSWindow>,

    /// 行所在的文档视图。
    list: Retained<NSView>,

    /// 装行的滚动视图。
    scroll: Retained<NSScrollView>,

    /// 当前的行控件，重建时先移除。
    rows: RefCell<Vec<Row>>,

    /// 一行都没有时的提示。
    empty: Retained<NSTextField>,

    /// 校验错误，不清除草稿。
    error: Retained<NSTextField>,

    /// 行控件的 target；建行时用。
    target: Retained<PreferencesTarget>,

    mtm: MainThreadMarker,
}

impl PunctuationEditor {
    pub(super) fn new(mtm: MainThreadMarker, target: &Retained<PreferencesTarget>) -> Self {
        let mut form = Layout::new(EDITOR_WIDTH, 18.0);
        let layout = &mut form;
        note_full(
            layout,
            mtm,
            "自定义映射优先于内置转换（含默认的引号配对与数字后标点规则）。原符号须是单个英文标点；转换文本留空表示敲它时原样上屏。",
        );
        let list = NSView::initWithFrame(mtm.alloc(), NSRect::ZERO);
        let scroll = NSScrollView::initWithFrame(mtm.alloc(), NSRect::ZERO);
        scroll.setHasVerticalScroller(true);
        scroll.setBorderType(NSBorderType::BezelBorder);
        scroll.setDrawsBackground(false);
        scroll.setDocumentView(Some(&list));
        layout.place(&scroll, PAGE_PADDING, layout.inner_width(), LIST_HEIGHT);
        layout.next_row(LIST_HEIGHT);
        let add = button(mtm, "＋ 添加一行", Setting::AddPunctuationRow, target);
        layout.place(&add, PAGE_PADDING, 110.0, ROW_HEIGHT);
        layout.next_row(ROW_HEIGHT);
        let save = button(mtm, "保存映射", Setting::SavePunctuationMap, target);
        let cancel = button(mtm, "取消", Setting::CancelPunctuationEdit, target);
        cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
        layout.place(&save, PAGE_PADDING, 130.0, ROW_HEIGHT);
        layout.place(&cancel, PAGE_PADDING + 145.0, 100.0, ROW_HEIGHT);
        layout.next_row(ROW_HEIGHT);
        let error = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        error.setTextColor(Some(&NSColor::systemRedColor()));
        layout.place(&error, PAGE_PADDING, layout.inner_width(), 32.0);
        layout.next_row(32.0);
        let empty = small_label(mtm, "还没有自定义映射，点「添加一行」开始。");
        list.addSubview(&empty);
        let height = form.height() + 18.0;
        let content = NSView::initWithFrame(
            mtm.alloc(),
            NSRect::new(NSPoint::ZERO, NSSize::new(EDITOR_WIDTH, height)),
        );
        form.finish(&content, height);
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                NSRect::new(NSPoint::ZERO, NSSize::new(EDITOR_WIDTH, height)),
                NSWindowStyleMask::Titled,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe {
            window.setReleasedWhenClosed(false);
        }
        window.setTitle(&NSString::from_str("标点映射"));
        window.setContentView(Some(&content));
        Self {
            window,
            list,
            scroll,
            rows: RefCell::new(Vec::new()),
            empty,
            error,
            target: target.clone(),
            mtm,
        }
    }

    /// 按配置里的映射以 sheet 打开；`parent` 是偏好设置主窗口。
    pub(super) fn open(&self, parent: &NSWindow, map: &BTreeMap<char, String>) {
        let rows: Vec<(String, String)> = map
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect();
        // 一行都没有时也摆一行空白，方便直接开始
        let rows = if rows.is_empty() {
            vec![(String::new(), String::new())]
        } else {
            rows
        };
        self.rebuild(&rows);
        self.error.setStringValue(&NSString::from_str(""));
        parent.beginSheet_completionHandler(&self.window, None);
        if let Some(first) = self.rows.borrow().first() {
            self.window.makeFirstResponder(Some(&first.key));
        }
    }

    pub(super) fn close(&self) {
        if let Some(parent) = self.window.sheetParent() {
            parent.endSheet(&self.window);
        }
        self.window.orderOut(None);
    }

    pub(super) fn set_error(&self, error: &str) {
        self.error.setStringValue(&NSString::from_str(error));
    }

    /// 添加一行空白并把焦点给它。
    pub(super) fn add_row(&self) {
        let mut rows = self.current_rows();
        if rows.len() >= MAX_PUNCTUATION_ROWS {
            return;
        }
        rows.push((String::new(), String::new()));
        self.rebuild(&rows);
        if let Some(last) = self.rows.borrow().last() {
            self.window.makeFirstResponder(Some(&last.key));
        }
    }

    /// 删掉第 `index` 行；草稿在控件里，先读出来再整表重建。
    pub(super) fn remove_row(&self, index: usize) {
        let mut rows = self.current_rows();
        if index >= rows.len() {
            return;
        }
        rows.remove(index);
        self.rebuild(&rows);
    }

    /// 从行控件读出映射并校验；错误信息带行号，显示在编辑器里。
    pub(super) fn draft(&self) -> Result<BTreeMap<char, String>, String> {
        let mut map = BTreeMap::new();
        for (index, (key, value)) in self.current_rows().iter().enumerate() {
            let number = index + 1;
            let key = key.trim();
            let value = value.trim();
            let Some(c) = key.chars().next() else {
                // 整行空白跳过；只填了转换文本的提示补上原符号
                if value.is_empty() {
                    continue;
                }
                return Err(format!("第 {number} 行填了转换文本但没有原符号"));
            };
            if key.chars().count() != 1 || !is_valid_map_key(c) {
                return Err(format!("第 {number} 行的原符号「{key}」须是单个英文标点"));
            }
            if value.chars().count() > MAX_MAP_TEXT_CHARS {
                return Err(format!(
                    "第 {number} 行的转换文本最多 {MAX_MAP_TEXT_CHARS} 个字符"
                ));
            }
            if map.insert(c, value.to_owned()).is_some() {
                return Err(format!("原符号 {c} 不止一行，请合并或删掉多余的行"));
            }
        }
        Ok(map)
    }

    fn current_rows(&self) -> Vec<(String, String)> {
        self.rows
            .borrow()
            .iter()
            .map(|row| {
                (
                    row.key.stringValue().to_string(),
                    row.value.stringValue().to_string(),
                )
            })
            .collect()
    }

    /// 按给定行重建行控件（加行 / 删行后行号变了，删按钮的 tag 全部重新分配）。
    fn rebuild(&self, rows: &[(String, String)]) {
        let mtm = self.mtm;
        for row in self.rows.borrow_mut().drain(..) {
            row.key.removeFromSuperview();
            row.value.removeFromSuperview();
        }
        self.empty.setHidden(!rows.is_empty());
        let width = EDITOR_WIDTH - 2.0 * PAGE_PADDING;
        let visible_height = self.scroll.contentSize().height.max(LIST_HEIGHT);
        let document_height = (ROW * rows.len() as f64).max(visible_height);
        let content_width = self.scroll.contentSize().width.min(width);
        self.list.setFrame(NSRect::new(
            NSPoint::ZERO,
            NSSize::new(content_width, document_height),
        ));
        self.empty.setFrame(NSRect::new(
            NSPoint::new(0.0, document_height - NOTE_HEIGHT),
            NSSize::new(content_width, NOTE_HEIGHT),
        ));
        let value_width = (content_width - VALUE_X - REMOVE_WIDTH - 8.0).max(80.0);
        let mut new_rows = Vec::with_capacity(rows.len());
        for (index, (key, value)) in rows.iter().enumerate() {
            let y = document_height - ROW * (index as f64 + 1.0) + 2.0;
            let key_field = text_field(mtm, Setting::PunctuationDraft, &self.target);
            key_field.setPlaceholderString(Some(&NSString::from_str("原符号")));
            key_field.setStringValue(&NSString::from_str(key));
            key_field.setFrame(NSRect::new(
                NSPoint::new(0.0, y),
                NSSize::new(KEY_WIDTH, ROW_HEIGHT),
            ));
            self.list.addSubview(&key_field);
            let arrow = NSTextField::labelWithString(&NSString::from_str("→"), mtm);
            arrow.setAlignment(NSTextAlignment::Center);
            arrow.setFrame(NSRect::new(
                NSPoint::new(ARROW_X, y),
                NSSize::new(ARROW_WIDTH, ROW_HEIGHT),
            ));
            self.list.addSubview(&arrow);
            let value_field = text_field(mtm, Setting::PunctuationDraft, &self.target);
            value_field.setPlaceholderString(Some(&NSString::from_str("转换文本")));
            value_field.setStringValue(&NSString::from_str(value));
            value_field.setFrame(NSRect::new(
                NSPoint::new(VALUE_X, y),
                NSSize::new(value_width, ROW_HEIGHT),
            ));
            self.list.addSubview(&value_field);
            let remove = button(
                mtm,
                "删",
                Setting::PunctuationRemoveRow(index),
                &self.target,
            );
            remove.setFrame(NSRect::new(
                NSPoint::new(content_width - REMOVE_WIDTH, y),
                NSSize::new(REMOVE_WIDTH, ROW_HEIGHT),
            ));
            self.list.addSubview(&remove);
            new_rows.push(Row {
                key: key_field,
                value: value_field,
            });
        }
        *self.rows.borrow_mut() = new_rows;
        // 非翻转坐标系的文档视图默认停在底部，滚回顶部让第一行可见
        let clip = self.scroll.contentView();
        clip.scrollToPoint(NSPoint::new(
            0.0,
            document_height - clip.bounds().size.height,
        ));
        self.scroll.reflectScrolledClipView(&clip);
    }
}
