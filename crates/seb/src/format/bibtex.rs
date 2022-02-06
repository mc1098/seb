use crate::{
    ast::{self, Biblio, BiblioResolver, QuotedString},
    Error, ErrorKind,
};

use super::Format;

use biblatex::Bibliography;

/// A type wrapper around [`String`] to represent a `BibTex` format string.
#[derive(Debug)]
pub struct BibTex(String);

impl Format for BibTex {
    fn new(val: String) -> Self {
        Self(val)
    }

    fn parse(self) -> Result<Result<Biblio, BiblioResolver>, Error> {
        let biblio = if self.0.is_empty() {
            Bibliography::new()
        } else {
            Bibliography::parse(&self.0)
                .filter(|b| b.len() != 0)
                .ok_or_else(|| {
                    Error::new(ErrorKind::Deserialize, "Unable to parse string as BibTeX")
                })?
        };
        let entries = biblio.into_iter().map(ast::Resolver::from).collect();
        Ok(Biblio::try_resolve(entries))
    }

    fn compose(ast: Biblio) -> Self {
        let s = ast
            .entries()
            .map(|entry| {
                format!(
                    "@{}{{{},\n{}}}\n",
                    compose_variant(entry),
                    entry.cite(),
                    compose_fields(&entry.fields())
                )
            })
            .collect::<String>();
        Self(s)
    }

    fn raw(self) -> String {
        self.0
    }

    fn name() -> &'static str {
        "BibTex"
    }

    fn ext() -> &'static str {
        "bib"
    }
}

const fn compose_variant(entry: &ast::Entry) -> &'static str {
    match entry {
        ast::Entry::Article(_) => "article",
        ast::Entry::Book(_) => "book",
        ast::Entry::Booklet(_) => "booklet",
        ast::Entry::BookChapter(_) | ast::Entry::BookPages(_) => "inbook",
        ast::Entry::BookSection(_) => "incollection",
        ast::Entry::InProceedings(_) => "inproceedings",
        ast::Entry::Manual(_) => "manual",
        ast::Entry::MasterThesis(_) => "masterthesis",
        ast::Entry::PhdThesis(_) => "phdthesis",
        ast::Entry::Other(_) => "misc",
        ast::Entry::Proceedings(_) => "proceedings",
        ast::Entry::TechReport(_) => "techreport",
        ast::Entry::Unpublished(_) => "unpublished",
    }
}

fn bibtex_esc(s: &str) -> String {
    format!("{{{s}}}")
}

fn compose_fields(fields: &[ast::Field<'_, '_>]) -> String {
    fields
        .iter()
        .map(|field| {
            format!(
                "    {} = {{{}}},\n",
                field.name.replace('_', ""),
                field.value.map_quoted(bibtex_esc)
            )
        })
        .collect()
}

impl From<biblatex::Entry> for ast::Resolver {
    fn from(entry: biblatex::Entry) -> Self {
        // Deconstruct to avoid cloning
        let biblatex::Entry {
            key: cite,
            entry_type,
            mut fields,
        } = entry;

        let mut resolver = match entry_type {
            biblatex::EntryType::Article => ast::Article::resolver_with_cite(cite),
            biblatex::EntryType::Book => ast::Book::resolver_with_cite(cite),
            biblatex::EntryType::Booklet => ast::Booklet::resolver_with_cite(cite),
            biblatex::EntryType::InCollection => ast::BookSection::resolver_with_cite(cite),
            biblatex::EntryType::InProceedings => ast::InProceedings::resolver_with_cite(cite),
            biblatex::EntryType::Manual => ast::Manual::resolver_with_cite(cite),
            biblatex::EntryType::MastersThesis => ast::MasterThesis::resolver_with_cite(cite),
            biblatex::EntryType::PhdThesis => ast::PhdThesis::resolver_with_cite(cite),
            biblatex::EntryType::TechReport | biblatex::EntryType::Report => {
                ast::TechReport::resolver_with_cite(cite)
            }
            _ => ast::Other::resolver_with_cite(cite),
        };

        for (name, value) in fields.drain() {
            if name == "booktitle" {
                resolver.book_title(value.into());
            } else {
                resolver.set_field(&name, value.into());
            }
        }

        resolver
    }
}

impl From<biblatex::Chunks> for QuotedString {
    fn from(chunks: biblatex::Chunks) -> Self {
        let parts = chunks
            .into_iter()
            .map(|c| match c {
                biblatex::Chunk::Verbatim(s) => (true, s),
                biblatex::Chunk::Normal(s) => (false, s),
            })
            .collect();

        Self::from_parts(parts)
    }
}

#[cfg(test)]
mod tests {

    use std::{borrow::Cow, collections::HashMap};

    use crate::ast::FieldQuery;

    use super::*;

    fn fields() -> Vec<ast::Field<'static, 'static>> {
        vec![ast::Field {
            name: Cow::Borrowed("author"),
            value: Cow::Owned(QuotedString::new("Me".to_owned())),
        }]
    }

    fn entries() -> Vec<ast::Entry> {
        vec![ast::Entry::Manual(ast::Manual {
            cite: "entry1".to_owned(),
            title: QuotedString::new("Test".to_owned()),
            optional: HashMap::from([("author".to_owned(), QuotedString::new("Me".to_owned()))]),
        })]
    }

    #[test]
    fn biblatex_verbatim_chunk_escape_is_corrected() {
        use biblatex::Chunk::{Normal, Verbatim};
        // This test is for the real use case when adding using ietf as the title field is often
        // something like:
        //
        // title = {{Hypertext Transfer Protocol (HTTP/1.1): Authentication}}
        //
        // Notice the double curly braces - the whole title is verbatim and should not be styled
        // differently...however biblatex parses the '/' as an escape and splits up the title into
        // a mix of Verbatim and Normal chunks (per biblatex types).
        //
        // In the From<Chunks> impl we try to correct this by merging the escaped chunks back into
        // the single verbatim for QuotedString.

        // To reduce noise lets reduce the above example to the core problem
        // biblatex will parse the following into the below chunks:
        //
        // "{(HTTP/1.1)}"
        let chunks = vec![
            Verbatim("(HTTP/".to_owned()),
            Normal("1".to_owned()),
            Verbatim(".".to_owned()),
            Normal("1".to_owned()),
            Verbatim(")".to_owned()),
        ];

        let qs = QuotedString::from(chunks);

        assert_eq!("{(HTTP/1.1)}", qs.map_quoted(|s| format!("{{{s}}}")));
    }

    #[test]
    fn parse_then_compose_bibtex() {
        let bibtex_str = include_str!("../../../../tests/data/bibtex1.bib");
        let bibtex = BibTex::new(bibtex_str.to_owned());
        let parsed = bibtex
            .parse()
            .unwrap()
            .expect("bibtex1.bib is a valid bibtex entry");

        let composed = BibTex::compose(parsed.clone());

        // we don't want to compare bibtex_str with composed raw as they can be different
        let parsed_two = composed
            .parse()
            .unwrap()
            .expect("second parse of composed bibtex1 should be valid");

        assert_eq!(parsed, parsed_two);
    }

    #[test]
    fn compose_fields_to_bibtex() {
        let fields = fields();
        let result = compose_fields(&fields);

        assert_eq!("    author = {Me},\n", result);
    }

    #[test]
    fn book_title_in_bibtex_should_be_booktitle() {
        let result = compose_fields(&[ast::Field {
            name: Cow::Borrowed("book_title"),
            value: Cow::Owned(QuotedString::new("value".to_owned())),
        }]);

        assert_eq!("    booktitle = {value},\n", result);
    }

    #[test]
    fn parse_booktitle_field_as_book_title() {
        let biblio = BibTex::new("@misc{cite, title={title},booktitle={Correct},}".to_owned())
            .parse()
            .expect("Valid BibTeX string")
            .expect("Valid entry fields");

        let entry = biblio.into_entries().remove(0);

        assert_eq!("Correct", &**entry.get_field("book_title").unwrap());
    }

    #[test]
    fn compose_to_bibtex() {
        let references = Biblio::new(entries().drain(..1).collect());
        let result = BibTex::compose(references);

        // indents and newlines are important in this string so don't format!
        let expected = "@manual{entry1,
    title = {Test},
    author = {Me},
}\n";

        assert_eq!(expected, result.raw());
    }
}
