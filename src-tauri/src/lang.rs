//! Translation target languages. This list is the single source of truth; the frontend reads
//! it through the `languages` command.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Ko,
    Ja,
    Zh,
    Es,
    Fr,
    De,
    Pt,
}

/// One entry of the `languages` command.
#[derive(Debug, Serialize, PartialEq)]
pub struct LangInfo {
    pub code: &'static str,
    pub name: &'static str,
    pub native: &'static str,
}

impl Lang {
    pub const ALL: [Lang; 7] = [
        Lang::Ko,
        Lang::Ja,
        Lang::Zh,
        Lang::Es,
        Lang::Fr,
        Lang::De,
        Lang::Pt,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Ko => "ko",
            Lang::Ja => "ja",
            Lang::Zh => "zh",
            Lang::Es => "es",
            Lang::Fr => "fr",
            Lang::De => "de",
            Lang::Pt => "pt",
        }
    }

    /// English name, also used inside prompts.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Ko => "Korean",
            Lang::Ja => "Japanese",
            Lang::Zh => "Chinese (Simplified)",
            Lang::Es => "Spanish",
            Lang::Fr => "French",
            Lang::De => "German",
            Lang::Pt => "Portuguese",
        }
    }

    pub fn native(self) -> &'static str {
        match self {
            Lang::Ko => "한국어",
            Lang::Ja => "日本語",
            Lang::Zh => "简体中文",
            Lang::Es => "Español",
            Lang::Fr => "Français",
            Lang::De => "Deutsch",
            Lang::Pt => "Português",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }

    /// The words this language's papers use in captions for "Figure" and "Table".
    pub(crate) fn caption_words(self) -> (&'static str, &'static str) {
        match self {
            Lang::Ko => ("그림", "표"),
            Lang::Ja => ("図", "表"),
            Lang::Zh => ("图", "表"),
            Lang::Es => ("Figura", "Tabla"),
            Lang::Fr => ("Figure", "Tableau"),
            Lang::De => ("Abbildung", "Tabelle"),
            Lang::Pt => ("Figura", "Tabela"),
        }
    }

    /// The written register a well-edited research text uses in this language.
    pub(crate) fn register(self) -> &'static str {
        match self {
            Lang::Ko => "polite formal style (합니다/입니다체)",
            Lang::Ja => {
                "the plain academic style of Japanese papers and textbooks (である調), not です/ます"
            }
            Lang::Zh => {
                "the written academic style (书面语) of Chinese papers and textbooks, in Simplified characters with full-width Chinese punctuation"
            }
            Lang::Es => "the neutral academic register of Spanish papers and textbooks",
            Lang::Fr => "the neutral academic register of French papers and textbooks",
            Lang::De => "the neutral academic register of German papers and textbooks",
            Lang::Pt => "the neutral academic register of Portuguese papers and textbooks",
        }
    }
}

/// Fixed replies of the AI paper finder, used when Claude gives none.
impl Lang {
    /// arXiv returned no candidates at all.
    pub fn no_papers_found(self) -> &'static str {
        match self {
            Lang::Ko => {
                "arXiv에서 맞는 논문을 찾지 못했어요. 주제를 조금 더 구체적으로 적어 주세요."
            }
            Lang::Ja => {
                "arXivで該当する論文が見つかりませんでした。テーマをもう少し具体的に書いてください。"
            }
            Lang::Zh => "在 arXiv 上没有找到相关论文。请把主题写得更具体一些。",
            Lang::Es => {
                "No encontré artículos que encajen en arXiv. Describe el tema con un poco más de detalle."
            }
            Lang::Fr => {
                "Aucun article correspondant sur arXiv. Décrivez le sujet un peu plus précisément."
            }
            Lang::De => {
                "Auf arXiv wurden keine passenden Arbeiten gefunden. Beschreiben Sie das Thema etwas genauer."
            }
            Lang::Pt => {
                "Não encontrei artigos correspondentes no arXiv. Descreva o tema com um pouco mais de detalhe."
            }
        }
    }

    /// Claude picked nothing and wrote no reply.
    pub fn no_pick(self) -> &'static str {
        match self {
            Lang::Ko => "맞는 논문을 고르지 못했어요. 다르게 물어봐 주세요.",
            Lang::Ja => "合う論文を選べませんでした。別の言い方で聞いてみてください。",
            Lang::Zh => "没能选出合适的论文。请换个说法再问一次。",
            Lang::Es => "No pude elegir un artículo adecuado. Prueba a preguntarlo de otra forma.",
            Lang::Fr => {
                "Je n'ai pas trouvé d'article adapté. Essayez de formuler la demande autrement."
            }
            Lang::De => {
                "Es ließ sich keine passende Arbeit auswählen. Formulieren Sie die Frage bitte anders."
            }
            Lang::Pt => "Não consegui escolher um artigo adequado. Tente perguntar de outra forma.",
        }
    }

    /// Claude picked papers but wrote no reply.
    pub fn picks_intro(self) -> &'static str {
        match self {
            Lang::Ko => "이 논문들을 추천해요.",
            Lang::Ja => "これらの論文をおすすめします。",
            Lang::Zh => "推荐以下论文。",
            Lang::Es => "Te recomiendo estos artículos.",
            Lang::Fr => "Je vous recommande ces articles.",
            Lang::De => "Diese Arbeiten kann ich empfehlen.",
            Lang::Pt => "Recomendo estes artigos.",
        }
    }
}

pub fn all() -> Vec<LangInfo> {
    Lang::ALL
        .into_iter()
        .map(|l| LangInfo {
            code: l.code(),
            name: l.name(),
            native: l.native(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_through_serde() {
        for l in Lang::ALL {
            let json = serde_json::to_string(&l).unwrap();
            assert_eq!(json, format!("\"{}\"", l.code()));
            assert_eq!(serde_json::from_str::<Lang>(&json).unwrap(), l);
            assert_eq!(Lang::from_code(l.code()), Some(l));
        }
    }

    #[test]
    fn unknown_codes_are_rejected() {
        for bad in ["\"en\"", "\"KO\"", "\"\"", "\"korean\"", "1"] {
            assert!(serde_json::from_str::<Lang>(bad).is_err(), "{bad}");
        }
        assert_eq!(Lang::from_code("en"), None);
    }

    #[test]
    fn finder_replies_exist_for_every_language() {
        for l in Lang::ALL {
            assert!(!l.no_papers_found().is_empty() && !l.no_pick().is_empty());
            assert!(!l.picks_intro().is_empty());
        }
        assert_eq!(Lang::Ko.picks_intro(), "이 논문들을 추천해요.");
    }

    #[test]
    fn list_order_and_default() {
        let codes: Vec<&str> = all().iter().map(|l| l.code).collect();
        assert_eq!(codes, ["ko", "ja", "zh", "es", "fr", "de", "pt"]);
        assert_eq!(Lang::default(), Lang::Ko);
        assert_eq!(all()[0].native, "한국어");
    }
}
