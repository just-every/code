//! Copyright 2015, Yuheng Chen. Apache 2 licensed.
//!
//! This vendored code used to be yaml-rust.  It's intended to be replaced in
//! the next major version with a yaml-rust2 which is an actively maintained
//! version of this.  Is it has different snapshot formats and different
//! MSRV requirements, we vendor it temporarily.

#![allow(unused)]

pub mod emitter;
pub mod parser;
pub mod scanner;
pub mod yaml;

pub use self::yaml::Yaml;

#[cfg(test)]
mod tests {
    use super::*;

    use crate::content::yaml::vendored::emitter::YamlEmitter;
    use crate::content::yaml::vendored::scanner::ScanError;
    use crate::content::yaml::vendored::yaml::YamlLoader;

    #[test]
    fn test_api() {
        let s = "
# from yaml-cpp example
- name: Ogre
  position: [0, 5, 0]
  powers:
    - name: Club
      damage: 10
    - name: Fist
      damage: 8
- name: Dragon
  position: [1, 0, 10]
  powers:
    - name: Fire Breath
      damage: 25
    - name: Claws
      damage: 15
- name: Wizard
  position: [5, -3, 0]
  powers:
    - name: Acid Rain
      damage: 50
    - name: Staff
      damage: 3
";
        let docs = YamlLoader::load_from_str(s).unwrap();
        let doc = &docs[0];

        assert_eq!(doc[0]["name"].as_str().unwrap(), "Ogre");

        let mut writer = String::new();
        {
            let mut emitter = YamlEmitter::new(&mut writer);
            emitter.dump(doc).unwrap();
        }

        assert!(!writer.is_empty());
    }

    fn try_fail(s: &str) -> Result<Vec<Yaml>, ScanError> {
        let t = YamlLoader::load_from_str(s)?;
        Ok(t)
    }

    #[test]
    fn test_fail() {
        let s = "
# syntax error
scalar
key: [1, 2]]
key1:a2
";
        assert!(YamlLoader::load_from_str(s).is_err());
        assert!(try_fail(s).is_err());
    }
}
