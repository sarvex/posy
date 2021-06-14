use crate::prelude::*;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Specifier {
    pub op: CompareOp,
    pub value: String,
}

impl Specifier {
    pub fn satisfied_by(&self, version: &Version) -> Result<bool> {
        Ok(self
            .op
            .to_ranges(&self.value)?
            .into_iter()
            .any(|r| r.contains(version)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Specifiers(pub Vec<Specifier>);

impl Specifiers {
    pub fn satisfied_by(&self, version: &Version) -> Result<bool> {
        for specifier in &self.0 {
            if !specifier.satisfied_by(&version)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl TryFrom<&str> for Specifiers {
    type Error = anyhow::Error;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        let specifiers_or_err = super::reqparse::versionspec(input);
        specifiers_or_err
            .map(|specifiers| Specifiers(specifiers))
            .with_context(|| {
                format!("failed to parse versions specifiers from {:?}", input)
            })
    }
}

try_from_str_boilerplate!(Specifiers);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CompareOp {
    LessThanEqual,
    StrictlyLessThan,
    NotEqual,
    Equal,
    GreaterThanEqual,
    StrictlyGreaterThan,
    Compatible,
}

fn parse_version_wildcard(input: &str) -> Result<(Version, bool)> {
    let (vstr, wildcard) = if let Some(vstr) = input.strip_suffix(".*") {
        (vstr, true)
    } else {
        (input, false)
    };
    let version: Version = vstr.try_into()?;
    Ok((version, wildcard))
}

/// Converts a comparison like ">= 1.2" into a union of [half, open) ranges.
///
/// Has to take a string, not a Version, because == and != can take "wildcards", which
/// are not valid versions.
// XX local version handling -- I think everything except == and != is supposed to error
// out if the rhs version has a local segment?
impl CompareOp {
    pub fn to_ranges(&self, rhs: &str) -> Result<Vec<Range<Version>>> {
        use CompareOp::*;
        let (version, wildcard) = parse_version_wildcard(rhs)?;
        Ok(if wildcard {
            if version.0.dev.is_some() || !version.0.local.is_empty() {
                bail!("version wildcards can't have dev or local suffixes");
            }
            // =~ X.* correspond to the half-open range
            //
            // [X.dev0, (X+1).dev0)
            let mut low = version.clone();
            low.0.dev = Some(0);
            let mut high = version.clone();
            // .* can actually appear after .postX or .aX, so we need to find the last
            // numeric entry in the version, and increment that.
            if let Some(post) = high.0.post {
                high.0.post = Some(post + 1)
            } else if let Some(pre) = high.0.pre {
                use pep440::PreRelease::*;
                high.0.pre = Some(match pre {
                    RC(n) => RC(n + 1),
                    A(n) => A(n + 1),
                    B(n) => B(n + 1),
                })
            } else {
                *high.0.release.last_mut().unwrap() += 1;
            }
            high.0.dev = Some(0);
            match self {
                Equal => vec![low..high],
                NotEqual => {
                    vec![Version::ZERO.clone()..low, high..Version::INFINITY.clone()]
                }
                _ => bail!("Can't use wildcard with {:?}", self),
            }
        } else {
            // no wildcards here
            if self != &Equal && self != &NotEqual {
                if !version.0.local.is_empty() {
                    bail!("Operator {:?} cannot be used on a version with a +local suffix");
                }
            }
            match self {
                // These two are simple
                LessThanEqual => vec![Version::ZERO.clone()..version.next()],
                GreaterThanEqual => vec![version.clone()..Version::INFINITY.clone()],
                // These are also pretty simple, because we took care of the wildcard
                // cases up above.
                Equal => vec![version.clone()..version.next()],
                NotEqual => vec![
                    Version::ZERO.clone()..version.clone(),
                    version.next()..Version::INFINITY.clone(),
                ],
                // "The exclusive ordered comparison >V MUST NOT allow a post-release of
                // the given version unless V itself is a post release."
                StrictlyGreaterThan => {
                    let mut low = version.clone();
                    if let Some(dev) = &version.0.dev {
                        low.0.dev = Some(dev + 1);
                    } else if let Some(post) = &version.0.post {
                        low.0.post = Some(post + 1);
                    } else {
                        // Otherwise, want to increment either the pre-release (a0 ->
                        // a1), or the "last" release segment. But working with
                        // pre-releases takes a lot of typing, and there is no "last"
                        // release segment -- X.Y.Z is just shorthand for
                        // X.Y.Z.0.0.0.0... So instead, we tack on a .post(INFINITY) and
                        // hope no-one actually makes a version like this in practice.
                        low.0.post = Some(u32::MAX);
                    }
                    vec![low..Version::INFINITY.clone()]
                }
                // "The exclusive ordered comparison <V MUST NOT allow a pre-release of
                // the specified version unless the specified version is itself a
                // pre-release."
                StrictlyLessThan => {
                    if (&version.0.pre, &version.0.dev) == (&None, &None) {
                        let mut new_max = version.clone();
                        new_max.0.dev = Some(0);
                        new_max.0.post = None;
                        new_max.0.local = vec![];
                        vec![Version::ZERO.clone()..new_max]
                    } else {
                        // Otherwise, some kind of pre-release
                        vec![Version::ZERO.clone()..version]
                    }
                }
                // ~= X.Y.suffixes is the same as >= X.Y.suffixes && == X.*
                // So it's a half-open range:
                //   [X.Y.suffixes, (X+1).dev0)
                Compatible => {
                    if version.0.release.len() < 2 {
                        bail!("~= operator requires a version with two segments (X.Y)");
                    }
                    let mut new_max = Version(pep440::Version {
                        epoch: version.0.epoch,
                        release: version.0.release.clone(),
                        pre: None,
                        post: None,
                        dev: Some(0),
                        local: vec![],
                    });
                    // Unwraps here are safe because we confirmed that the vector has at
                    // least 2 elements above.
                    new_max.0.release.pop().unwrap();
                    *new_max.0.release.last_mut().unwrap() += 1;
                    vec![version..new_max]
                }
            }
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::from_commented_json;

    #[test]
    fn test_invalid_specifiers_table() {
        let examples: Vec<String> =
            from_commented_json(include_str!("test-data/invalid-specifiers.txt"));

        fn chew_on(example: &str) -> Result<Specifiers> {
            let specs: Specifiers = example.try_into()?;
            // We only detect some problems when trying to actually use the specifier
            for spec in &specs.0 {
                spec.op.to_ranges(&spec.value)?;
            }
            Ok(specs)
        }

        for example in examples {
            println!("Parsing {:?}", example);
            let got = chew_on(&example);
            println!("Got {:?}", got);
            assert!(got.is_err());
        }
    }

    #[test]
    fn test_successful_specifiers_table() {
        let examples: Vec<(String, String)> =
            from_commented_json(include_str!("test-data/successful-specifiers.txt"));

        for (version_str, spec_str) in examples {
            println!("Matching {:?} against {:?}", version_str, spec_str);
            let version: Version = version_str.try_into().unwrap();
            let specs: Specifiers = spec_str.try_into().unwrap();
            println!("{:?}", specs.0[0].op.to_ranges(&specs.0[0].value));
            assert!(specs.satisfied_by(&version).unwrap());
        }
    }

    #[test]
    fn test_failing_specifiers_table() {
        let examples: Vec<(String, String)> =
            from_commented_json(include_str!("test-data/failing-specifiers.txt"));

        for (version_str, spec_str) in examples {
            println!("Matching {:?} against {:?}", version_str, spec_str);
            let version: Version = version_str.try_into().unwrap();
            let specs: Specifiers = spec_str.try_into().unwrap();
            println!("{:?}", specs.0[0].op.to_ranges(&specs.0[0].value));
            assert!(!specs.satisfied_by(&version).unwrap());
        }
    }
}
