use std::{collections::HashMap, fs::File, io::BufReader, path::PathBuf};

use celes::Country;
use geo::{point, Contains, Geometry};
use geojson::{FeatureCollection, GeoJson};

use crate::COUNTRY_FILE;

pub struct CountryLookup {
    countries: HashMap<String, CountryFeature>,
}

impl CountryLookup {
    pub fn new() -> Self {
        let root: PathBuf = project_root::get_project_root().unwrap();
        let path = root.join(COUNTRY_FILE);
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);

        let geojson = GeoJson::from_reader(reader).unwrap();
        let features = FeatureCollection::try_from(geojson).unwrap();
        
        let mut countries = HashMap::new();

        for feature in features.features.iter() {
            let properties = feature.properties.clone().unwrap();
            let iso_a2 = properties.get("iso_a2").unwrap().as_str().unwrap();
            if iso_a2 == "-99" {
                continue;
            }
           // println!("ISO A2: {} - {}", iso_a2, properties.get("name").unwrap().as_str().unwrap());
            let Ok(country) = Country::from_alpha2(iso_a2) else {
                continue;
            };
            let country_feature = CountryFeature {
                country: country.clone(),
                polygon: Geometry::try_from(feature.geometry.clone().unwrap()).unwrap()
            };
            countries.insert(iso_a2.to_string(), country_feature);
        }

        Self {
            countries
        }
    }
    
    pub fn get_country(&self, lat: f64, lon: f64, previous: Option<String>) -> Option<String> {
        let pt = point!(x: lon, y: lat);

        // Check if the previous country is still valid
        if let Some(previous_country) = previous {
            if let Some(country_feature) = self.countries.get(&previous_country) {
                if country_feature.polygon.contains(&pt) {
                    return Some(country_feature.country.alpha2.to_owned());
                }
            }
        }

        for country_feature in self.countries.values() {
            if country_feature.polygon.contains(&pt) {
                return Some(country_feature.country.alpha2.to_owned());
            }
        }

        None
    }
}

struct CountryFeature {
    country: Country,
    polygon: Geometry,
}

#[test]
fn test_country_lookup() {
    let before_load = std::time::Instant::now();
    let country_lookup = CountryLookup::new();
    let after_load = std::time::Instant::now();

    // DK
    let lat = 55.;
    let lon = 9.;
    let country1 = country_lookup.get_country(lat, lon, Some("DK".to_owned()));
    let after_lookup1 = std::time::Instant::now();

    // AM
    let lat = 40.664208;
    let lng = 44.873029;
    let country2 = country_lookup.get_country(lat, lng, None);

    let after_lookup2 = std::time::Instant::now();

    println!("Load: {:?}", after_load.duration_since(before_load));
    println!("Lookup known: {:?}", after_lookup1.duration_since(after_load));
    println!("Lookup unknown: {:?}", after_lookup2.duration_since(after_lookup1));

    println!("Countries found: {:?}, {:?}", country1, country2);
}