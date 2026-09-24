//! 中文模式下的全角标点。
//!
//! 组句期间 `,` `.` 是翻页键、`-` `=` 也是，这些由壳决定；这里只回答「这个半角字符在中文模式下该变成什么」。
//! 默认映射之外，壳可用 [`Punctuation::set_custom_map`] 叠加用户配置的覆盖（见 `[general] punctuation_map`），
//! 覆盖优先于默认映射；覆盖值给空串表示这个键不转换。

use std::collections::BTreeMap;

/// 标点映射覆盖表里转换文本的长度上限（字符数），保存与读取同一套。
pub const MAX_MAP_TEXT_CHARS: usize = 16;

/// 标点映射覆盖表的键是否合法：必须是 ASCII 标点。字母数字不行——它们进组句或选候选，
/// 轮不到标点转换；翻页键在组句中仍翻页，只有组句外才走映射。
pub fn is_valid_map_key(c: char) -> bool {
    c.is_ascii_punctuation()
}

/// 标点映射覆盖表的共用校验（保存与读取同一套）：键是单个 ASCII 标点、值不超
/// [`MAX_MAP_TEXT_CHARS`] 个字符；空串合法——它表示「该键不转换」。
pub fn validate_map(map: &BTreeMap<char, String>) -> Result<(), String> {
    for (key, value) in map {
        if !is_valid_map_key(*key) {
            return Err(format!(
                "原输入符号 {key} 必须是 ASCII 标点（字母数字不行）"
            ));
        }
        if value.chars().count() > MAX_MAP_TEXT_CHARS {
            return Err(format!("{key} 的转换文本最多 {MAX_MAP_TEXT_CHARS} 个字符"));
        }
    }
    Ok(())
}

/// 引号成对切换与自定义映射覆盖。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Punctuation {
    /// 下一个 `"` 是左引号。
    double_quote_open: bool,

    /// 下一个 `'` 是左引号。
    single_quote_open: bool,

    /// 上一个上屏字符是 ASCII 数字：`3.14` 里的 `.` 保持半角。
    after_digit: bool,

    /// 用户配置的覆盖：合并盖在默认映射上，值是上屏文本；空串表示该键不转换。
    custom: BTreeMap<char, String>,
}

impl Punctuation {
    /// 半角字符对应的全角标点；不需要转换的返回 `None`，壳把原字符交给应用。
    pub fn convert(&mut self, c: char) -> Option<String> {
        // 覆盖优先：命中即返回（空串 = 不转换），默认的引号配对与「数字后的点」规则都不再走
        if let Some(text) = self.custom.get(&c) {
            return if text.is_empty() {
                None
            } else {
                Some(text.clone())
            };
        }
        let after_digit = std::mem::replace(&mut self.after_digit, false);
        let converted = match c {
            ',' => "，",
            '.' if after_digit => return None,
            '.' => "。",
            '?' => "？",
            '!' => "！",
            ':' => "：",
            ';' => "；",
            '(' => "（",
            ')' => "）",
            '[' => "【",
            ']' => "】",
            '{' => "「",
            '}' => "」",
            '<' => "《",
            '>' => "》",
            '/' => "、",
            '\\' => "、",
            '^' => "……",
            '_' => "——",
            '$' => "￥",
            '~' => "～",
            '"' => {
                self.double_quote_open = !self.double_quote_open;
                if self.double_quote_open { "“" } else { "”" }
            }
            '\'' => {
                self.single_quote_open = !self.single_quote_open;
                if self.single_quote_open { "‘" } else { "’" }
            }
            _ => return None,
        };
        Some(converted.to_owned())
    }

    /// 用户配置的标点映射覆盖；键值合法性见 [`validate_map`]，空值条目照收——它表示「这个键不转换」。
    pub fn set_custom_map(&mut self, map: &BTreeMap<char, String>) {
        self.custom = map.clone();
    }

    /// 只重置随输入走的运行状态（引号配对、数字记忆），保留配置的自定义映射；
    /// 隐私边界丢弃输入（`Engine::discard_input`）与挂起会话切换时用。
    pub fn reset_transient(&mut self) {
        self.double_quote_open = false;
        self.single_quote_open = false;
        self.after_digit = false;
    }

    /// 只交换随输入走的运行状态，自定义映射是配置、各会话共用，不跟着换。
    pub fn swap_transient(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.double_quote_open, &mut other.double_quote_open);
        std::mem::swap(&mut self.single_quote_open, &mut other.single_quote_open);
        std::mem::swap(&mut self.after_digit, &mut other.after_digit);
    }

    /// 壳把没有转换的字符原样交给应用后调用，用来记住「刚打了数字」。
    pub fn note_passthrough(&mut self, c: char) {
        self.after_digit = c.is_ascii_digit();
    }

    /// 有文本上屏后调用（候选、拼音、英文词），数字状态按最后一个字符更新。
    pub fn note_committed(&mut self, text: &str) {
        self.after_digit = text.chars().last().is_some_and(|c| c.is_ascii_digit());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_common_marks_and_toggles_quotes() {
        let mut p = Punctuation::default();
        assert_eq!(p.convert(','), Some("，".to_owned()));
        assert_eq!(p.convert('"'), Some("“".to_owned()));
        assert_eq!(p.convert('"'), Some("”".to_owned()));
        assert_eq!(p.convert('\''), Some("‘".to_owned()));
        assert_eq!(p.convert('\''), Some("’".to_owned()));
        assert_eq!(p.convert('a'), None);
        assert_eq!(p.convert('-'), None);
    }

    #[test]
    fn slash_and_braces_have_defaults() {
        let mut p = Punctuation::default();
        assert_eq!(p.convert('/'), Some("、".to_owned()));
        assert_eq!(p.convert('\\'), Some("、".to_owned()));
        assert_eq!(p.convert('{'), Some("「".to_owned()));
        assert_eq!(p.convert('}'), Some("」".to_owned()));
        assert_eq!(p.convert('['), Some("【".to_owned()));
    }

    #[test]
    fn period_after_digit_stays_ascii() {
        let mut p = Punctuation::default();
        p.note_passthrough('3');
        assert_eq!(p.convert('.'), None);
        assert_eq!(p.convert('.'), Some("。".to_owned()));
        p.note_committed("第1");
        assert_eq!(p.convert('.'), None);
        p.note_committed("开发");
        assert_eq!(p.convert('.'), Some("。".to_owned()));
    }

    #[test]
    fn map_validation_rejects_bad_keys_and_long_values() {
        assert!(
            validate_map(&BTreeMap::from([
                ('/', "、".to_owned()),
                ('^', String::new()),
            ]))
            .is_ok()
        );
        assert!(validate_map(&BTreeMap::from([('a', "×".to_owned())])).is_err());
        assert!(validate_map(&BTreeMap::from([('。', "！".to_owned())])).is_err());
        assert!(
            validate_map(&BTreeMap::from([(
                '~',
                "太".repeat(MAX_MAP_TEXT_CHARS + 1)
            )]))
            .is_err()
        );
    }

    #[test]
    fn custom_map_overrides_defaults_and_empty_disables() {
        let mut p = Punctuation::default();
        p.set_custom_map(&BTreeMap::from([
            ('/', "／".to_owned()),
            ('^', String::new()),
            ('x', "×".to_owned()),
        ]));
        assert_eq!(p.convert('/'), Some("／".to_owned()));
        // 空串：该键不转换，也不影响其他默认映射
        assert_eq!(p.convert('^'), None);
        // 覆盖了默认的引号配对：直接出映射值，不成对切换
        p.set_custom_map(&BTreeMap::from([('"', "„".to_owned())]));
        assert_eq!(p.convert('"'), Some("„".to_owned()));
        assert_eq!(p.convert('"'), Some("„".to_owned()));
        // 没覆盖的默认映射照旧
        assert_eq!(p.convert(','), Some("，".to_owned()));
    }

    #[test]
    fn custom_map_survives_transient_resets() {
        let mut p = Punctuation::default();
        p.set_custom_map(&BTreeMap::from([('/', "／".to_owned())]));
        p.reset_transient();
        assert_eq!(p.convert('/'), Some("／".to_owned()));
        // 挂起会话切换只换运行状态，映射不动
        let mut other = Punctuation::default();
        other.set_custom_map(&BTreeMap::from([('/', "／".to_owned())]));
        p.note_passthrough('5');
        p.swap_transient(&mut other);
        assert_eq!(p.convert('.'), Some("。".to_owned()));
        assert_eq!(other.convert('.'), None);
    }
}
