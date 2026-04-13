use git2::{BranchType, ObjectType};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OidProxy(pub String);

impl OidProxy {
    pub fn new(oid: git2::Oid) -> Self {
        Self(oid.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureProxy {
    pub name: String,
    pub email: String,
    pub time: i64,
    // UTC offset in minutes
    pub offset: i32,
}

impl SignatureProxy {
    pub fn new(sig: &git2::Signature) -> Self {
        Self {
            name: sig.name().unwrap_or("").to_string(),
            email: sig.email().unwrap_or("").to_string(),
            time: sig.when().seconds(),
            offset: sig.when().offset_minutes(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitProxy {
    pub oid: OidProxy,
    pub tree_oid: OidProxy,
    pub author: SignatureProxy,
    pub committer: SignatureProxy,
    pub message: String,
    pub parent_oids: Vec<OidProxy>,
}

impl CommitProxy {
    pub fn new(commit: &git2::Commit) -> Self {
        Self {
            oid: OidProxy::new(commit.id()),
            tree_oid: OidProxy::new(commit.tree_id()),
            author: SignatureProxy::new(&commit.author()),
            committer: SignatureProxy::new(&commit.committer()),
            message: commit.message().unwrap_or("").to_string(),
            parent_oids: commit.parent_ids().map(OidProxy::new).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeProxy {
    pub oid: OidProxy,
}

impl TreeProxy {
    pub fn new(tree: &git2::Tree) -> Self {
        Self {
            oid: OidProxy::new(tree.id()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceProxy {
    pub name: String,
    pub target_oid: Option<OidProxy>,
}

impl ReferenceProxy {
    pub fn new(reference: &git2::Reference) -> Self {
        Self {
            name: reference.name().unwrap_or("").to_string(),
            target_oid: reference.target().map(OidProxy::new),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectProxy {
    pub oid: OidProxy,
    pub kind: Option<ObjectType>,
}

impl ObjectProxy {
    pub fn new(obj: &git2::Object) -> Self {
        Self {
            oid: OidProxy::new(obj.id()),
            kind: obj.kind(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchProxy {
    pub name: String,
    pub branch_type: BranchType,
    pub get_obj: Option<ObjectProxy>,
}

impl BranchProxy {
    pub fn new(branch: &git2::Branch, branch_type: BranchType) -> Self {
        Self {
            name: branch.name().ok().flatten().unwrap_or("").to_string(),
            branch_type,
            get_obj: branch
                .get()
                .peel(ObjectType::Any)
                .ok()
                .map(|o| ObjectProxy::new(&o)),
        }
    }
}
