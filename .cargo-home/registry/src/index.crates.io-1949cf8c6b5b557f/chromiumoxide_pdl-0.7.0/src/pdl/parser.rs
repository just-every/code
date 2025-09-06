//! Parser for the chrome pdl files
//!
//! All regexp's are copied from pdl.py in the chromium source tree.
use crate::pdl::dep::is_circular_dep;
use crate::pdl::*;
use std::borrow::Cow;
use std::fmt;

/// Helper macro to create `&'static Regex`
macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

#[derive(Debug)]
pub struct Error {
    pub message: String,
}
impl Error {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

macro_rules! format_err {
    ($($tt:tt)*) => {
        $crate::pdl::parser::Error {
            message: format!($($tt)*),
        }
    };
}

macro_rules! bail {
    ($($tt:tt)*) => { return Err(format_err!($($tt)*)) };
}

macro_rules! borrowed {
    ($m:expr) => {
        $m.map(|x|x.as_str()).map(std::borrow::Cow::Borrowed)
    };
    ($m:expr, $($tt:tt)*) => {
         borrowed!($m).ok_or_else(||format_err!($($tt)*))
    };
}

/// Parse the input into a [`Protocol`].
///
/// Rewrite of the Python script from the Chromium source tree.
///
///  See: https://chromium.googlesource.com/deps/inspector_protocol/+/refs/heads/master/pdl.py
pub fn parse_pdl(input: &str) -> Result<Protocol, Error> {
    let mut protocol = Protocol::default();
    let mut description: Option<String> = None;
    let mut version = None;

    // type, command, event
    let mut element: Option<Element> = None;
    // parameters, properties, returns
    let mut member: Option<Member> = None;
    let mut member_enum = false;

    for (idx, line) in input.lines().enumerate() {
        let line_num = idx + 1;

        let trim_line = line.trim();
        if trim_line.starts_with('#') {
            if let Some(desc) = description.as_mut() {
                desc.push('\n');
                desc.extend(trim_line.chars().skip(1).skip_while(|c| c.is_whitespace()));
            } else {
                description = Some(
                    trim_line
                        .chars()
                        .skip(1)
                        .skip_while(|c| c.is_whitespace())
                        .collect::<String>(),
                );
            }
            continue;
        }

        if trim_line.is_empty() {
            continue;
        }

        if let Some(caps) = regex!("^(experimental )?(deprecated )?domain (.*)").captures(line) {
            if let Some(domain) = protocol.domains.last_mut() {
                if let Some(mut element) = element.take() {
                    if let Some(member) = member.take() {
                        element.add_member(member)?;
                    }
                    element.consume(domain);
                }
            }

            let domain = Domain {
                description: description.take().map(Cow::Owned),
                experimental: caps.get(1).is_some(),
                deprecated: caps.get(2).is_some(),
                name: borrowed!(caps.get(3), "line {}: No name for domain", line_num)?,
                dependencies: vec![],
                types: vec![],
                commands: vec![],
                events: vec![],
            };
            protocol.domains.push(domain);
            continue;
        }

        if let Some(caps) = regex!("^  depends on ([^\\s]+)").captures(line) {
            protocol
                .domains
                .last_mut()
                .ok_or_else(|| format_err!("line {}: missing domain declaration", line_num))?
                .dependencies
                .push(borrowed!(caps.get(1)).unwrap());
            continue;
        }

        // type
        if let Some(caps) =
            regex!("^  (experimental )?(deprecated )?type (.*) extends (array of )?([^\\s]+)")
                .captures(line)
        {
            let domain = protocol
                .domains
                .last_mut()
                .ok_or_else(|| format_err!("line {}: missing domain declaration", line_num))?;

            if let Some(mut el) = element.take() {
                if let Some(member) = member.take() {
                    el.add_member(member)?;
                }
                el.consume(domain);
            }
            let name = borrowed!(caps.get(3)).unwrap();
            let ty = TypeDef {
                description: description.take().map(Cow::Owned),
                experimental: caps.get(1).is_some(),
                deprecated: caps.get(2).is_some(),
                raw_name: Cow::Owned(format!("{}.{}", domain.name, name)),
                is_circular_dep: is_circular_dep(&domain.name, name.as_ref()),
                name,
                extends: Type::new(caps.get(5).unwrap().as_str(), caps.get(4).is_some()),
                item: None,
            };
            element = Some(Element::Type(ty));
            continue;
        }

        // cmd or event
        if let Some(caps) =
            regex!("^  (experimental )?(deprecated )?(command|event) (.*)").captures(line)
        {
            let domain = protocol
                .domains
                .last_mut()
                .ok_or_else(|| format_err!("line {}: missing domain declaration", line_num))?;
            if let Some(mut el) = element.take() {
                if let Some(member) = member.take() {
                    el.add_member(member)?;
                }
                el.consume(domain);
            }
            let name = borrowed!(caps.get(4)).unwrap();
            if Some("command") == caps.get(3).map(|m| m.as_str()) {
                let cmd = Command {
                    description: description.take().map(Cow::Owned),
                    experimental: caps.get(1).is_some(),
                    deprecated: caps.get(2).is_some(),
                    parameters: vec![],
                    returns: vec![],
                    redirect: None,
                    raw_name: Cow::Owned(format!("{}.{}", domain.name, name)),
                    is_circular_dep: is_circular_dep(&domain.name, name.as_ref()),
                    name,
                };
                element = Some(Element::Commnad(cmd));
            } else {
                let ev = Event {
                    description: description.take().map(Cow::Owned),
                    experimental: caps.get(1).is_some(),
                    deprecated: caps.get(2).is_some(),
                    parameters: vec![],
                    raw_name: Cow::Owned(format!("{}.{}", domain.name, name)),
                    is_circular_dep: is_circular_dep(&domain.name, name.as_ref()),
                    name,
                };
                element = Some(Element::Event(ev));
            };
            continue;
        }

        // member to params / returns / properties
        if let Some(caps) = regex!(
            "^      (experimental )?(deprecated )?(optional )?(array of )?([^\\s]+) ([^\\s]+)"
        )
        .captures(line)
        {
            let domain = protocol
                .domains
                .last_mut()
                .ok_or_else(|| format_err!("line {}: missing domain declaration", line_num))?;
            let name = borrowed!(caps.get(6)).unwrap();
            let param = Param {
                description: description.take().map(Cow::Owned),
                experimental: caps.get(1).is_some(),
                deprecated: caps.get(2).is_some(),
                optional: caps.get(3).is_some(),
                raw_name: Cow::Owned(format!("{}.{}", domain.name, name)),
                is_circular_dep: is_circular_dep(&domain.name, name.as_ref()),
                name,
                r#type: Type::new(caps.get(5).unwrap().as_str(), caps.get(4).is_some()),
            };
            match member.as_mut().ok_or_else(|| {
                format_err!(
                    "line {}: parameter {} has no declared member section",
                    line_num,
                    param.name
                )
            })? {
                Member::Parameters(params) => params.push(param),
                Member::Returns(params) => params.push(param),
                Member::Properties(params) => params.push(param),
            };
            if Some("enum") == caps.get(5).map(|m| m.as_str()) {
                member_enum = true;
            }
            continue;
        }

        // parameters, returns, properties definition
        if let Some(caps) = regex!("^    (parameters|returns|properties)").captures(line) {
            if let Some(member) = member.take() {
                element
                    .as_mut()
                    .ok_or_else(|| format_err!("line {}: member has no parent item", line_num))?
                    .add_member(member)?;
            }
            match caps.get(1).unwrap().as_str() {
                "parameters" => member = Some(Member::Parameters(vec![])),
                "returns" => member = Some(Member::Returns(vec![])),
                "properties" => member = Some(Member::Properties(vec![])),
                _ => unreachable!(),
            }
            continue;
        }

        // enum
        if line.starts_with("    enum") {
            member_enum = false;
            if let Some(Element::Type(ref mut ty)) = element.as_mut() {
                if ty.item.is_none() {
                    ty.item = Some(Item::Enum(vec![]));
                    continue;
                } else {
                    bail!("line {}: enum declaration not allowed", line_num);
                }
            } else {
                bail!("line {}: enum declaration not allowed", line_num);
            }
        }

        // version
        if line.starts_with("version") {
            protocol.description = description.take().map(Cow::Owned);
            version = Some(Version::default());
            continue;
        }

        if let Some(caps) = regex!("^  major (\\d+)").captures(line) {
            let v = version
                .as_mut()
                .ok_or_else(|| format_err!("line {}: version must be declared first", line_num))?;
            v.major = caps.get(1).unwrap().as_str().parse().unwrap();
            continue;
        }

        if let Some(caps) = regex!("^  minor (\\d+)").captures(line) {
            let v = version
                .as_mut()
                .ok_or_else(|| format_err!("line {}: missing version declaration", line_num))?;
            v.minor = caps.get(1).unwrap().as_str().parse().unwrap();
            continue;
        }

        // redirect
        if let Some(caps) = regex!("^    redirect ([^\\s]+)").captures(line) {
            let mut redirect = Redirect {
                description: description.take().map(Cow::Owned),
                domain: borrowed!(caps.get(1)).unwrap(),
                name: None,
            };
            if let Some(desc) = description.as_ref() {
                if let Some(caps) = regex!("^Use '([^']+)' instead$").captures(desc) {
                    let name = caps.get(1).unwrap().as_str();
                    redirect.name = name.rsplit('.').next().map(str::to_string).map(Cow::Owned);
                }
            }
            match element
                .as_mut()
                .ok_or_else(|| format_err!("line {}: missing item declaration", line_num))?
            {
                Element::Commnad(cmd) => {
                    cmd.redirect = Some(redirect);
                }
                _ => bail!("line {}: can't add redirect here", line_num),
            }
            continue;
        }

        // enum literal
        if regex!("^      (  )?[^\\n\\t]+$").is_match(line) {
            if member_enum {
                let param = match member
                    .as_mut()
                    .ok_or_else(|| format_err!("line {}: missing member declaration", line_num))?
                {
                    Member::Parameters(params) => params.last_mut(),
                    Member::Returns(params) => params.last_mut(),
                    Member::Properties(params) => params.last_mut(),
                }
                .ok_or_else(|| format_err!("line {}: missing parameter declaration", line_num))?;

                if let Type::Enum(ref mut vars) = param.r#type {
                    vars.push(Variant {
                        description: description.take().map(Cow::Owned),
                        name: Cow::Borrowed(trim_line),
                    });
                } else {
                    bail!("line {}: missing enum declaration", line_num)
                }
            } else {
                match element
                    .as_mut()
                    .ok_or_else(|| format_err!("line {}: missing item declaration", line_num))?
                {
                    Element::Type(ty) => {
                        if let Some(Item::Enum(vars)) = ty.item.as_mut() {
                            vars.push(Variant {
                                description: description.take().map(Cow::Owned),
                                name: Cow::Borrowed(trim_line),
                            });
                        } else {
                            bail!("line {}: missing enum declaration", line_num)
                        }
                    }
                    _ => bail!("line {}: missing enum declaration", line_num),
                }
            }
            continue;
        }
        bail!("line {}: unknown token `{}`", line_num, line)
    }

    if let Some(domain) = protocol.domains.last_mut() {
        if let Some(mut element) = element.take() {
            if let Some(member) = member.take() {
                element.add_member(member)?;
            }
            element.consume(domain);
        }
    }

    protocol.version = version.ok_or_else(|| format_err!("Missing version"))?;
    Ok(protocol)
}

#[derive(Debug)]
enum Member<'a> {
    Parameters(Vec<Param<'a>>),
    Returns(Vec<Param<'a>>),
    Properties(Vec<Param<'a>>),
}
#[derive(Debug)]
enum Element<'a> {
    Type(TypeDef<'a>),
    Commnad(Command<'a>),
    Event(Event<'a>),
}

impl<'a> Element<'a> {
    fn consume(self, domain: &mut Domain<'a>) {
        match self {
            Element::Type(ty) => domain.types.push(ty),
            Element::Commnad(cmd) => domain.commands.push(cmd),
            Element::Event(ev) => domain.events.push(ev),
        }
    }

    fn add_member(&mut self, member: Member<'a>) -> Result<(), Error> {
        match member {
            Member::Parameters(params) => match self {
                Element::Commnad(cmd) => {
                    cmd.parameters = params;
                    return Ok(());
                }
                Element::Event(ev) => {
                    ev.parameters = params;
                    return Ok(());
                }
                _ => {}
            },
            Member::Returns(params) => {
                if let Element::Commnad(cmd) = self {
                    cmd.returns = params;
                    return Ok(());
                }
            }
            Member::Properties(params) => {
                if let Element::Type(ty) = self {
                    if ty.item.is_some() {
                        bail!("Type {} can't have additional properties section", ty.name)
                    } else {
                        ty.item = Some(Item::Properties(params));
                        return Ok(());
                    }
                }
            }
        }

        Err(format_err!("Invalid member"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_protocol() {
        let s = r#"# Copyright 2017 The Chromium Authors. All rights reserved.
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

version
  major 1
  minor 2

experimental domain DummyDomain
  depends on DOM

  # node identifier.
  type NodeId extends string

  # Enum of possible property types.
  type SomeValueType extends string
    enum
      boolean
      undefined
      booleanOrUndefined

  # Console Message.
  type ConsoleMessage extends object
    properties
      # Message source.
      enum source
        xml
        javascript
        network
      # Message severity.
      enum level
        log
        warning
        info
      # Message text.
      string text
      # URL of the message origin.
      optional string url
      # Line number in the resource that generated this message (1-based).
      optional integer line
      # Column number in the resource that generated this message (1-based).
      optional integer column
"#;

        parse_pdl(s).unwrap();
    }
}
