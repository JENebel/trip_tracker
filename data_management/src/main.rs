use std::collections::HashSet;

use chrono::{FixedOffset, TimeZone};
use clap::{Parser, Subcommand};
use data_management::{database::db::TripDatabase, geonames::CountryLookup, DataManager};

#[derive(Parser)]
#[command(name = "TripCLI")]
#[command(about = "A CLI to update trips and sessions", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set the title of a trip
    Ttitl { trip_id: i64, new_title: String },
    /// Set the description of a trip
    Tdesc {
        trip_id: i64,
        new_description: String,
    },
    /// Set the title of a session
    Stitl { session_id: i64, new_title: String },
    /// Set the description of a session
    Sdesc {
        session_id: i64,
        new_description: String,
    },
    /// Hide a session
    Hide { session_id: i64 },
    /// Unhide a session
    Unhide { session_id: i64 },
    /// List sessions in trip
    List { trip_id: i64 },
    /// Combine the 2 sessions into 1, and hide the original sessions.
    /// The sessions will inherit metadata from the first session
    Combine {
        trip_id: i64,
        session_id_1: i64,
        session_id_2: i64,
    },
    /// Print the api key for the trip
    ApiKey {
        trip_ip: i64,
    },
    /// Force end a session. BE CAREFUL
    Ends {
        session_id: i64,
    },
    RedoCountries {
        trip_id: i64
    },
    AddGpx {
        trip_id: i64,
        gpx_file: String,
        title: String,
    },
    ExportGpx {
        session_id: i64,
    },
    FixTime {
        session_id: i64,
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let db = TripDatabase::connect().await.unwrap();

    match &cli.command {
        Commands::Ttitl { trip_id, new_title } => {
            db.set_trip_title(*trip_id, new_title).await.unwrap();
        },
        Commands::Tdesc {trip_id, new_description} => {
            db.set_trip_description(*trip_id, new_description)
                .await
                .unwrap();
        },
        Commands::Stitl {session_id, new_title} => {
            db.set_session_title(*session_id, new_title)
                .await
                .unwrap();
        },
        Commands::Sdesc {session_id, new_description} => {
            db.set_session_description(*session_id, new_description)
                .await
                .unwrap();
        },
        Commands::Hide { session_id } => {
            let session = db.get_session(*session_id).await.unwrap();
            if !session.active {
                db.set_session_hidden(*session_id, true).await.unwrap()
            }
        },
        Commands::Unhide { session_id } => {
            db.set_session_hidden(*session_id, false).await.unwrap()
        },
        Commands::List { trip_id } => {
            let trip = db.get_trip(*trip_id).await.unwrap();
            println!("{}", trip.title);
            let sessions = db.get_trip_sessions(*trip_id).await.unwrap();
            for session in sessions {
                let time_str = if session.track_points.len() > 0 {
                    let ts = session.track_points[0].timestamp;
                    FixedOffset::east_opt(2 * 3600).unwrap().from_utc_datetime(&ts.naive_utc()).format("%d/%m/%Y %H:%M (UTC+2)").to_string()
                } else {
                    "-".to_string()
                };
                println!("{}\t{}\t{}\t{}", session.session_id, if session.active {"A"} else if session.hidden {"H"} else {"."}, time_str, session.title)
            }
        },
        Commands::Combine {trip_id, session_id_1, session_id_2} => {
            let session1 = db.get_session(*session_id_1).await.unwrap();
            let session2 = db.get_session(*session_id_2).await.unwrap();

            if session1.active || session2.active {
                panic!("Both sessions must be inactive to combine!")
            }

            let mut track_points = Vec::new();
            if session1.start_time < session2.start_time {
                track_points.extend(session1.track_points);
                track_points.extend(session2.track_points);
            } else {
                track_points.extend(session2.track_points);
                track_points.extend(session1.track_points);
            }

            let session = db.insert_track_session(*trip_id, session1.title.clone(), session1.description.clone(), session1.start_time.clone(), session1.active).await.unwrap();
            db.set_session_track_points(session.session_id, track_points).await.unwrap();

            db.set_session_hidden(*session_id_1, true).await.unwrap();
            db.set_session_hidden(*session_id_2, true).await.unwrap();
        },
        Commands::ApiKey { trip_ip } => {
            println!("{}", db.get_trip(*trip_ip).await.unwrap().api_token)
        },
        Commands::Ends { session_id } => {
            db.set_session_active(*session_id, false).await.unwrap()
        },
        Commands::RedoCountries { trip_id } => {
            let mut countries = HashSet::new(); 

            let mut prev_country = None;
            let ids = db.get_nonhidden_trip_session_ids(*trip_id).await.unwrap();

            let country_lookup = CountryLookup::new();

            for id in ids {
                let session = db.get_session(id).await.unwrap();
                for point in session.track_points {
                    let country = country_lookup.get_country(point.latitude, point.longitude, prev_country.clone());
                    if let Some(country) = &country {
                        if !countries.contains(country) {
                            countries.insert(country.clone());
                        }
                    }
                    prev_country = country;
                }
            }

            db.set_trip_countries(*trip_id, countries.into_iter().collect()).await.unwrap();
        },
        Commands::AddGpx { trip_id, gpx_file, title } => {
            let data_manager = DataManager::start().await.unwrap();
            data_manager.add_gpx_to_trip(gpx_file, *trip_id, Some(title)).await.unwrap();
        },
        Commands::ExportGpx { session_id } => {
            let data_manager = DataManager::start().await.unwrap();
            data_manager.export_gpx(*session_id).await;
        },
        Commands::FixTime { session_id } => {
            let session = db.get_session(*session_id).await.unwrap();
            
            let start_time = session.start_time;
            let point_time = session.track_points[0].timestamp;
            let offset = point_time.signed_duration_since(start_time);

            let track_points = session.track_points.iter().map(|p| {let mut p = p.clone(); p.timestamp = start_time + offset; p}).collect::<Vec<_>>();

            let new_session = db.insert_track_session(session.trip_id, session.title.clone(), session.description.clone(), session.start_time.clone(), session.active).await.unwrap();
            db.set_session_track_points(new_session.session_id, track_points).await.unwrap();

            db.set_session_hidden(*session_id, true).await.unwrap();
        }
    }

    println!("Success!")
}
