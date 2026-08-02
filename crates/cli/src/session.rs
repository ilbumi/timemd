//! `today` and `log` — time that has already been spent.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use timemd_core::day::Session;
use timemd_core::{ProjectSlug, Result, Store};

use crate::{name_or_dash, suffix};

pub fn today(store: &Store, date: Option<NaiveDate>, now: NaiveDateTime) -> Result<String> {
    let date = date.unwrap_or_else(|| now.date());
    let day = store.read_day(date)?;

    let mut lines = vec![format!("{date} — {} tracked", day.total())];
    for session in day.sessions() {
        lines.push(format!(
            "  {}-{} {:>7}  {}{}",
            session.start.format("%H:%M"),
            session.end.format("%H:%M"),
            session.duration().to_string(),
            name_or_dash(session.project.as_ref()),
            suffix(&session.note),
        ));
    }
    for problem in day.problems() {
        lines.push(format!("  ! {problem}"));
    }
    Ok(lines.join("\n"))
}

pub fn log(
    store: &Store,
    project: Option<String>,
    from: NaiveTime,
    to: NaiveTime,
    note: String,
    date: Option<NaiveDate>,
    now: NaiveDateTime,
) -> Result<String> {
    let date = date.unwrap_or_else(|| now.date());
    let session = Session::new(from, to, project.map(ProjectSlug::new).transpose()?, note);
    let logged = session.duration();
    store.update_day(date, |day| day.add_session(session))?;
    Ok(format!("logged {logged} on {date}"))
}

#[cfg(test)]
mod tests {
    use crate::testing::{at, log, moment, store};
    use crate::{Command, run};
    use chrono::NaiveDate;
    use timemd_core::Minutes;

    #[test]
    fn log_writes_a_session_without_the_timer() {
        let (_directory, store) = store();

        let logged = run(
            &store,
            Command::Log {
                project: Some("timemd".to_owned()),
                from: at(14, 0),
                to: at(15, 30),
                note: "meeting".to_owned(),
                date: None,
            },
            moment(16, 0),
        )
        .expect("logs");

        assert!(logged.contains("1h30m"), "{logged}");
        assert_eq!(
            store.read_day(moment(9, 0).date()).expect("reads").total(),
            Minutes::new(90)
        );
    }

    #[test]
    fn log_accepts_an_explicit_date() {
        let (_directory, store) = store();
        let earlier = NaiveDate::from_ymd_opt(2026, 7, 20).expect("valid date");

        run(
            &store,
            log(None, at(9, 0), at(10, 0), Some(earlier)),
            moment(16, 0),
        )
        .expect("logs");

        assert_eq!(
            store.read_day(earlier).expect("reads").total(),
            Minutes::new(60)
        );
    }

    #[test]
    fn log_rejects_an_invalid_slug() {
        let (_directory, store) = store();
        assert!(
            run(
                &store,
                log(Some("Nope!"), at(9, 0), at(10, 0), None),
                moment(9, 0)
            )
            .is_err()
        );
    }

    #[test]
    fn today_lists_sessions_and_the_total() {
        let (_directory, store) = store();
        run(
            &store,
            Command::Log {
                project: Some("timemd".to_owned()),
                from: at(9, 0),
                to: at(9, 25),
                note: "work".to_owned(),
                date: None,
            },
            moment(10, 0),
        )
        .expect("logs");

        let output = run(&store, Command::Today { date: None }, moment(10, 0)).expect("reads");
        assert!(output.contains("25m tracked"), "{output}");
        assert!(output.contains("09:00-09:25"), "{output}");
        assert!(output.contains("timemd"), "{output}");
    }

    #[test]
    fn today_surfaces_lines_it_could_not_read() {
        let (_directory, store) = store();
        let path = store.day_path(moment(9, 0).date());
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("creates dir");
        std::fs::write(
            &path,
            "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- nonsense\n",
        )
        .expect("writes");

        let output = run(&store, Command::Today { date: None }, moment(10, 0)).expect("reads");
        assert!(output.contains("  ! "), "{output}");
    }
}
