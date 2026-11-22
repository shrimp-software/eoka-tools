use std::fmt;

use crate::eoka::BoundingBox;

#[derive(Debug, Clone)]
pub struct InteractiveElement {
    pub index: usize,
    pub tag: String,
    pub role: Option<String>,
    pub text: String,
    pub placeholder: Option<String>,
    pub input_type: Option<String>,
    pub selector: String,
    pub checked: bool,
    pub disabled: bool,
    pub visible: bool,
    pub value: Option<String>,
    pub bbox: BoundingBox,
}

impl fmt::Display for InteractiveElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] <{}", self.index, self.tag)?;
        if let Some(ref t) = self.input_type {
            if t != "text" {
                write!(f, " type=\"{}\"", t)?;
            }
        }
        f.write_str(">")?;
        if self.checked {
            f.write_str(" [checked]")?;
        }
        if !self.text.is_empty() {
            write!(f, " \"{}\"", self.text)?;
        }
        if let Some(ref v) = self.value {
            write!(f, " value=\"{}\"", v)?;
        }
        if let Some(ref p) = self.placeholder {
            write!(f, " placeholder=\"{}\"", p)?;
        }
        if let Some(ref r) = self.role {
            let redundant = (r == "button" && self.tag == "button")
                || (r == "link" && self.tag == "a")
                || (r == "menuitem" && self.tag == "a");
            if !redundant {
                write!(f, " role=\"{}\"", r)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ObserveConfig {
    pub viewport_only: bool,
}

impl Default for ObserveConfig {
    fn default() -> Self {
        Self {
            viewport_only: true,
        }
    }
}

#[cfg(test)]
mod tests;
