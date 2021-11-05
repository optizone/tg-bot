use crate::{ALLIAS_REGIONS, ALL_TAGS};
use std::collections::HashSet;

pub enum Regions<'t> {
    Regions(Vec<&'t str>),
    BadRegion {
        region: &'t str,
        matches: Vec<&'static str>,
    },
}

pub fn extract_regions<'t>(regions: &'t str) -> Regions<'t> {
    let alias_regions = ALLIAS_REGIONS.read().unwrap();
    let mut res = Vec::new();
    for region in regions.split_whitespace() {
        if let Some(&reg) = alias_regions.get(region.to_lowercase().as_str()) {
            res.push(reg);
        } else if region.to_lowercase() == "страна" {
            return Regions::Regions(vec![]);
        } else {
            let mut it = alias_regions
                .iter()
                .filter(|(a, _)| a.starts_with(&region.to_lowercase()));
            let first = it.next();
            let mut other = it.map(|(_, r)| *r).collect::<HashSet<_>>();
            if !other.is_empty() && !other.iter().all(|r| r == first.unwrap_or((&"", &"")).1)
                || first.is_none()
            {
                first.map(|(_, r)| other.insert(*r));
                let mut matches = other.iter().copied().collect::<Vec<_>>();
                matches.sort_unstable();
                return Regions::BadRegion { region, matches };
            }
            res.push(*first.unwrap().1);
        }
    }
    Regions::Regions(res)
}

pub enum Tags<'t> {
    Tags(Vec<&'static str>),
    BadTag(&'t str),
}

pub fn extract_tags<'t>(tags: &'t str) -> Tags<'t> {
    let all_tags = ALL_TAGS.read().unwrap();
    let mut res = Vec::new();
    for tag in tags.trim().split_whitespace() {
        match all_tags.get(tag.to_uppercase().as_str()) {
            Some(tag) => res.push(*tag),
            None => return Tags::BadTag(tag),
        }
    }
    Tags::Tags(res)
}
