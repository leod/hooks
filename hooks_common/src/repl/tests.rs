use std::io::Cursor;

use bit_manager::{BitWriter, BitReader, BitWrite, BitRead};

use repl;

#[derive(Component, PartialEq, Clone, BitStore)]
pub struct A {
    pub x: f32,
}

#[derive(Component, PartialEq, Clone, BitStore)]
pub struct B {
    pub x: u16,
    pub y: bool,
}

snapshot! {
    use super::A;
    use super::B;

    mod snap {
        a: A,
        b: B,
    }
}

#[test]
fn test_delta() {
    // Entity classes
    let class_none = snap::EntityClass {
        components: vec![]
    };
    let class_a = snap::EntityClass {
        components: vec![snap::ComponentType::A]
    };
    let class_b = snap::EntityClass {
        components: vec![snap::ComponentType::B]
    };
    let class_a_b = snap::EntityClass {
        components: vec![snap::ComponentType::A, snap::ComponentType::B]
    };

    let classes = {
        let mut classes = snap::EntityClasses::new();
        classes.0.insert(1, class_none);
        classes.0.insert(2, class_a);
        classes.0.insert(3, class_b);
        classes.0.insert(7, class_a_b);
        classes
    };

    // Meta-information about entities
    let repl_none = repl::Entity {
        owner: 0,
        class_id: 1
    };
    let repl_a = repl::Entity {
        owner: 0,
        class_id: 2
    };
    let repl_b = repl::Entity {
        owner: 0,
        class_id: 3
    };
    let repl_a_b = repl::Entity {
        owner: 0,
        class_id: 7
    };

    // Some entity snapshots.
    // Note: in normal usage, these would not be constructed by hand.
    let entity_a1 = snap::EntitySnapshot {
        a: Some(A { x: 42.3 }),
        b: None,
    };
    let entity_a2 = snap::EntitySnapshot {
        a: Some(A { x: 666.0 }),
        b: None,
    };
    let entity_b = snap::EntitySnapshot {
        a: None,
        b: Some(
            B {
                x: 128,
                y: true
            }),
    };
    let entity_ab = snap::EntitySnapshot {
        a: Some(A { x: 1.0 }),
        b: Some(
            B { 
                x: 256,
                y: true
            }),
    };

    // Snapshots
    let snapshot_empty = snap::WorldSnapshot::new();
    assert!(snapshot_empty.0.len() == 0);

    let snapshot_a1 = {
        let mut snapshot = snap::WorldSnapshot::new();
        snapshot.0.insert(42, (repl_a.clone(), entity_a1.clone()));
        snapshot
    };
    assert!(snapshot_a1.0.len() == 1);

    let snapshot_a2 = {
        let mut snapshot = snap::WorldSnapshot::new();
        snapshot.0.insert(42, (repl_a.clone(), entity_a2.clone()));
        snapshot
    };
    assert!(snapshot_a2.0.len() == 1);

    let snapshot_aa = {
        let mut snapshot = snap::WorldSnapshot::new();
        snapshot.0.insert(42, (repl_a.clone(), entity_a1.clone()));
        snapshot.0.insert(1,  (repl_a.clone(), entity_a2.clone()));
        snapshot
    };
    assert!(snapshot_aa.0.len() == 2);


    // Testing
    {
        let repr_empty_a1 = {
            let mut writer = BitWriter::new(Vec::new());
            snapshot_empty.delta_write(&snapshot_a1, &classes, 0, &mut writer).unwrap();

            writer.into_inner().unwrap()
        };
        let repr_empty_a2 = {
            let mut writer = BitWriter::new(Vec::new());
            snapshot_empty.delta_write(&snapshot_a2, &classes, 0, &mut writer).unwrap();

            writer.into_inner().unwrap()
        };

        let (new_entities, snapshot) = {
            let mut reader = BitReader::new(Cursor::new(&repr_empty_a1));

            snapshot_empty.delta_read(&classes, &mut reader).unwrap()
        };
        assert!(new_entities == vec![42]);
        assert!(snapshot == snapshot_a1);

        let (new_entities, snapshot) = {
            let mut reader = BitReader::new(Cursor::new(&repr_empty_a2));

            snapshot_empty.delta_read(&classes, &mut reader).unwrap()
        };
        assert!(new_entities == vec![42]);
        assert!(snapshot == snapshot_a2);
    }

    {
        let repr_a1_a2 = {
            let mut writer = BitWriter::new(Vec::new());
            snapshot_a1.delta_write(&snapshot_a2, &classes, 0, &mut writer).unwrap();

            writer.into_inner().unwrap()
        };
        let repr_a1_a1 = {
            let mut writer = BitWriter::new(Vec::new());
            snapshot_a1.delta_write(&snapshot_a1, &classes, 0, &mut writer).unwrap();

            writer.into_inner().unwrap()
        };
        
        // Nothing should be written if snapshot is the same
        assert!(repr_a1_a1 == [0, 0, 0, 0]);

        let (new_entities, snapshot) = {
            let mut reader = BitReader::new(Cursor::new(&repr_a1_a2));

            snapshot_a1.delta_read(&classes, &mut reader).unwrap()
        };
        assert!(new_entities == vec![]);
        assert!(snapshot == snapshot_a2);

        let (new_entities, snapshot) = {
            let mut reader = BitReader::new(Cursor::new(&repr_a1_a1));

            snapshot_a1.delta_read(&classes, &mut reader).unwrap()
        };
        assert!(new_entities == vec![]);
        assert!(snapshot == snapshot_a1);
    }

    {
        let repr_a1_aa = {
            let mut writer = BitWriter::new(Vec::new());
            snapshot_a1.delta_write(&snapshot_aa, &classes, 0, &mut writer).unwrap();

            writer.into_inner().unwrap()
        };
        let repr_a2_aa = {
            let mut writer = BitWriter::new(Vec::new());
            snapshot_a2.delta_write(&snapshot_aa, &classes, 0, &mut writer).unwrap();

            writer.into_inner().unwrap()
        };

        let (new_entities, snapshot) = {
            let mut reader = BitReader::new(Cursor::new(&repr_a1_aa));

            snapshot_a1.delta_read(&classes, &mut reader).unwrap()
        };
        assert!(new_entities == vec![1]);
        assert!(snapshot == snapshot_aa);

        // Using the wrong base snapshot should yield the wrong result
        let (new_entities, snapshot) = {
            let mut reader = BitReader::new(Cursor::new(&repr_a1_aa));

            snapshot_a2.delta_read(&classes, &mut reader).unwrap()
        };
        assert!(new_entities == vec![1]);
        assert!(snapshot != snapshot_aa);
    }
}
