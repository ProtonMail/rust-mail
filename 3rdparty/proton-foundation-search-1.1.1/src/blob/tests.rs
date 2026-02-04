use super::*;

#[test]
fn events_are_send() {
    fn _sendy(_subject: impl Send) {}
    fn _load(event: LoadEvent) {
        _sendy(event)
    }
    fn _save(event: SaveEvent) {
        _sendy(event)
    }
}

#[test]
fn events_are_sync() {
    fn _syncy(_subject: impl Sync) {}
    fn _load(event: LoadEvent) {
        _syncy(event)
    }
    fn _save(event: SaveEvent) {
        _syncy(event)
    }
}

#[test]
fn unique_id() {
    let blob_a = BlobId::generate("a");
    let blob_b = BlobId::generate("b");
    assert_ne!(blob_a, blob_b);
}

#[test]
#[should_panic(expected = "load event was not handled")]
fn load_unhandled_panics() {
    let mut blob = Blob::<()>::new(BlobId::new(1, 2), "test blob");
    let e = blob.fetch_and_free();
    let ControlFlow::Break(_) = e else {
        panic!("unexpected {e:?}");
    };
    let _e = blob.fetch_and_free();
    // should panic on fetch_and_free now
}

#[test]
fn load_empty() {
    let mut blob = Blob::<()>::new(BlobId::new(1, 2), "test blob");
    let e = blob.fetch_and_free();
    let ControlFlow::Break(load_event) = e else {
        panic!("unexpected {e:?}");
    };
    let cached = load_event.send_empty().expect("send empty");
    assert_eq!(cached.data.downcast().expect("downcast"), Arc::new(()));
    let e = blob.fetch_and_free();
    let ControlFlow::Continue(_v) = e else {
        panic!("unexpected {e:?}");
    };
}

#[test]
fn load_cached() {
    let mut blob = Blob::<()>::new(BlobId::new(1, 2), "test blob");
    let e = blob.fetch_and_free();
    let ControlFlow::Break(load_event) = e else {
        panic!("unexpected {e:?}");
    };
    load_event
        .send_cached(&Cached::new("blobby", Arc::new(())))
        .expect("send cached");
    let e = blob.fetch_and_free();
    let ControlFlow::Continue(_v) = e else {
        panic!("unexpected {e:?}");
    };
}

#[test]
#[should_panic(expected = "load event was not handled")]
fn load_cached_invalid() {
    let mut blob = Blob::<()>::new(BlobId::new(1, 2), "test blob");
    let e = blob.fetch_and_free();
    let ControlFlow::Break(load_event) = e else {
        panic!("unexpected {e:?}");
    };
    let error = load_event
        .send_cached(&Cached::new("blobby", Arc::new("wrong")))
        .expect_err("send wrong cached");
    assert_eq!(
        error.downcast().expect("error downcast"),
        Box::new(InvalidCast {
            want: "()",
            want_description: "test blob".into(),
            got: "&str",
            got_description: "blobby".into()
        })
    );
    let _ = blob.fetch_and_free();
    // should panic on fetch_and_free now
}

#[test]
fn load_serialized() {
    let mut blob = Blob::<()>::new(BlobId::new(1, 2), "test blob");
    let e = blob.fetch_and_free();
    let ControlFlow::Break(load_event) = e else {
        panic!("unexpected {e:?}");
    };
    let cached = load_event.send(&SerDes::Json, b"null").expect("send");
    assert_eq!(cached.data.downcast().expect("downcast"), Arc::new(()));
    let e = blob.fetch_and_free();
    let ControlFlow::Continue(_v) = e else {
        panic!("unexpected {e:?}");
    };
}

#[test]
fn save_recv() {
    let mut blob = Blob::<()>::new(BlobId::new(0, 1), "test blob");

    let (release, save) = blob.save_and_free(BlobId::generate("haf"), Arc::new(()));

    assert_eq!(release.expect("rel").id(), BlobId::new(0, 1));
    assert_eq!(save.id().to_string(), "AE2859AF98485FA1A694D8D63F37F97A");
    let cached = save.recv();
    let serialized = cached.serialize(&SerDes::JsonPretty).expect("recv");
    assert_eq!(serialized, b"null");
    assert_eq!(cached.data.downcast().expect("downcast"), Arc::new(()));
}

#[test]
fn save_cached() {
    let mut blob = Blob::<()>::new(BlobId::new(0, 1), "test blob");

    let (release, save) = blob.save_and_free(BlobId::generate("haf"), Arc::new(()));

    assert_eq!(release.expect("rel").id(), BlobId::new(0, 1));
    assert_eq!(save.id().to_string(), "AE2859AF98485FA1A694D8D63F37F97A");
    let cached = save.recv();
    assert_eq!(cached.data.downcast().expect("downcast"), Arc::new(()));
}
