# pdfcraft-xfdf

Layer L4: comment and form data exchange (execution plan M5.6, M6.7).

```rust
let xml = export_xfdf(&doc, true, true, "form.pdf");   // comments and/or field values
let fdf = export_fdf(&doc, false, true, "form.pdf")?;
let txt = export_data(&doc, Format::Txt);               // or Xml, Csv
let report = import(&mut doc, &bytes)?;                 // XFDF, FDF, XML, CSV or text, detected
```

- **XFDF** (ISO 19444-1): every comment type PdfCraft knows (text, free text, line, square,
  circle, polygon, polyline, markup, stamp, caret, ink, attachments, sound, redact) with rect,
  name, author, subject, dates, flags, colours, opacity, width, icon, quads, line ends, ink,
  default appearance, callouts (`callout`, `fringe`, `intent`, `head`), pop-ups and reply threads
  (`inreplyto`); field values nested by name, with text preserved across XML comments and processing instructions.
- **FDF**: fields (with `/Kids` hierarchies) and indirect comments. Nested annotation values
  are copied under FDF object numbers; shared references and cycles remain shared. Selection
  retains shared annotation objects until the budgeted copy, so aliases do not clone their
  values before the comment-count limit is checked. Retained keys and values are budgeted before
  copying; omitted links/appearances are filtered without cloning their payloads. Document
  links and appearances are excluded. Export returns `Result` and refuses graphs above
  100,000 comments/objects/values, 64 direct nesting levels or a conservative 64 MiB serialization
  budget instead of exporting partial data. Unknown indirect streams retain their encoded bytes.
  These limits cover the copied graph; source parsing and page discovery retain their existing
  COS/parser limits, with no total source-memory bound. FDF import semantics are unchanged.
- **Form data**: Acrobat's XML (`xfdf:original` keeps names that aren't XML names), CSV and
  tab-delimited text (a row of names, a row of values).

Import replaces comments with the same `/NM` (and drops the replaced comment's pop-up), draws
appearances PdfCraft can draw, and sets values through the form's own checks (formats,
validation, recalculation); read-only fields take imported values, as in Acrobat. XML is read
with `roxmltree`.
