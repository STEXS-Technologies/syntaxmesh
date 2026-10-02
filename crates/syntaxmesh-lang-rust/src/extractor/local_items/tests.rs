use super::super::*;

#[test]
fn local_function_containment_tracks_immediate_source_owners() -> Result<(), ExtractorError> {
    let extraction = RustExtractor.extract(&tests::source(
        "fn outer() { fn inner() { fn deeper() {} } mod local { fn separate() {} } } struct Service; impl Service { fn run() { fn inner() {} } } trait Contract { fn run() { fn inner() {} } }",
    ))?;
    for (owner, child) in [
        ("outer", "outer::inner"),
        ("outer::inner", "outer::inner::deeper"),
        ("Service::run", "Service::run::inner"),
        ("Contract::run", "Contract::run::inner"),
    ] {
        let owner = extraction.nodes.iter().find(|node| node.name == owner);
        let child = extraction.nodes.iter().find(|node| node.name == child);
        let (Some(owner), Some(child)) = (owner, child) else {
            return Err(ExtractorError::InvalidInput(
                "missing local declaration".to_owned(),
            ));
        };
        if extraction
            .edges
            .iter()
            .filter(|edge| {
                edge.source == owner.id
                    && edge.target == child.id
                    && edge.relation == RelationKind::Contains
                    && edge.provenance == child.provenance
            })
            .count()
            != 1
        {
            return Err(ExtractorError::InvalidInput(
                "incorrect local containment".to_owned(),
            ));
        }
    }
    for edge in &extraction.edges {
        if extraction
            .nodes
            .iter()
            .any(|node| node.id == edge.target && node.name == "outer::local::separate")
        {
            return Err(ExtractorError::InvalidInput(
                "crossed local module boundary".to_owned(),
            ));
        }
    }
    Ok(())
}

#[test]
fn local_function_calls_belong_to_their_own_declarations() -> Result<(), ExtractorError> {
    let extraction = RustExtractor.extract(&tests::source(
        "fn outer() { fn inner() { utility(); fn deeper() { other(); } } inner(); } fn utility() {} fn other() {}",
    ))?;
    let expected = [
        ("outer", "inner"),
        ("outer::inner", "utility"),
        ("outer::inner::deeper", "other"),
    ];
    if extraction.references.len() != expected.len() {
        return Err(ExtractorError::InvalidInput(
            "nested calls duplicated or lost".to_owned(),
        ));
    }
    for (reference, (owner, target)) in extraction.references.iter().zip(expected) {
        if reference.target != target
            || !extraction
                .nodes
                .iter()
                .any(|node| node.id == reference.source && node.name == owner)
        {
            return Err(ExtractorError::InvalidInput(
                "local call attributed to wrong declaration".to_owned(),
            ));
        }
    }
    Ok(())
}

#[test]
fn local_declarations_in_impl_and_trait_bodies_do_not_inherit_outer_call_context()
-> Result<(), ExtractorError> {
    let extraction = RustExtractor.extract(&tests::source(
        "struct Service; impl Service { fn run(&self) { fn local() { self.build(); } local(); } fn build(&self) {} } trait Contract { fn run(&self) { fn local() { utility(); } local(); } } fn utility() {}",
    ))?;
    for (owner, target) in [
        ("Service::run::local", "self.build"),
        ("Contract::run::local", "utility"),
    ] {
        if !extraction.references.iter().any(|reference| {
            reference.target == target
                && extraction
                    .nodes
                    .iter()
                    .any(|node| node.id == reference.source && node.name == owner)
        }) {
            return Err(ExtractorError::InvalidInput(
                "local impl/trait declaration lost ownership".to_owned(),
            ));
        }
    }
    if extraction.references.len() != 4 {
        return Err(ExtractorError::InvalidInput(
            "local impl/trait body calls duplicated".to_owned(),
        ));
    }
    Ok(())
}
