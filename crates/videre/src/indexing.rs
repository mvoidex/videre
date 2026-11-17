//! Bringing a known list of files into the library: what `watch` does for the
//! files an event named, and what `dedupe undo` does for the files it put
//! back.

use std::path::PathBuf;

/// Hash those of `paths` that need it (new or changed since their row was
/// written), store their rows, and reconcile their XMP sidecars. Returns the
/// paths written.
pub fn index_paths(
    conn: &rusqlite::Connection,
    library: &videre_core::library::LibraryContext,
    paths: Vec<PathBuf>,
    prec: videre_core::marks::XmpPrecedence,
    silent: bool,
) -> anyhow::Result<Vec<PathBuf>> {
    use rayon::prelude::*;
    let sigs = videre_core::db::stored_signatures(conn).unwrap_or_default();
    let paths: Vec<_> = paths
        .into_iter()
        .filter(|p| videre::incremental::needs_processing(&sigs, p))
        .collect();
    let records: Vec<videre::types::FileRecord> = paths
        .par_iter()
        .filter_map(|path| videre::hasher::hash_file_in(library, path).ok())
        .collect();
    videre::sqlite_output::write_records_in(conn, library, &records)?;
    videre::sqlite_output::resolve_unresolved(conn)?;
    videre::sqlite_output::orient_unoriented(conn)?;
    let changed: std::collections::HashSet<String> =
        records.iter().map(|r| r.path.clone()).collect();
    crate::xmp::reconcile_xmp_in(conn, library, prec, &changed, silent)?;
    Ok(records.iter().map(|r| PathBuf::from(&r.path)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_indexing_ignores_appledouble_but_keeps_a_real_dot_underscore_jpeg() {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let library = videre_core::library::LibraryContext::new(root.path(), cache.path()).unwrap();
        let conn = videre_core::library_db::initialize(&library).unwrap();
        let photo = library.paths.root.join("._çiçek.jpg");
        image::RgbImage::new(2, 2).save(&photo).unwrap();
        let sidecar = library.paths.root.join("._başka.jpg");
        let mut header = [0; 26];
        header[..4].copy_from_slice(&0x0005_1607u32.to_be_bytes());
        header[4..8].copy_from_slice(&0x0002_0000u32.to_be_bytes());
        std::fs::write(&sidecar, header).unwrap();

        let written = index_paths(
            &conn,
            &library,
            vec![photo.clone(), sidecar.clone()],
            videre_core::marks::XmpPrecedence::Db,
            true,
        )
        .unwrap();

        assert_eq!(written, vec![photo]);
        assert_eq!(
            conn.query_row("SELECT count(*) FROM file_hashes", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(std::fs::read(sidecar).unwrap(), header);
    }

    #[test]
    fn index_paths_stores_rows_for_exactly_the_given_files() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let home = tempfile::tempdir().unwrap();
        let library = videre_core::library::LibraryContext::new(&root, home.path()).unwrap();
        let conn = videre_core::library_db::initialize(&library).unwrap();
        let given = [root.join("çiçek.jpg"), root.join("Fotoğraf 1.jpg")];
        let other = root.join("dokunulmadı.jpg");
        for (i, p) in given.iter().chain([&other]).enumerate() {
            std::fs::write(p, format!("bayt {i}")).unwrap();
        }

        let written = index_paths(
            &conn,
            &library,
            given.to_vec(),
            videre_core::marks::XmpPrecedence::Db,
            true,
        )
        .unwrap();

        assert_eq!(written.len(), 2);
        let stored: Vec<String> = conn
            .prepare("SELECT path FROM file_hashes ORDER BY path")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let mut want: Vec<String> = given.iter().map(|p| p.to_string_lossy().into()).collect();
        want.sort();
        assert_eq!(stored, want);
    }
}
