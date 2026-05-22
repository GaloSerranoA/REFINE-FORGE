use refineforge_derive::LeanModel;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Right {
    Read,
    Write,
    Admin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, LeanModel)]
pub struct Capability {
    read: bool,
    write: bool,
    admin: bool,
    revoked: bool,
}

impl Capability {
    pub fn fresh(rights: &[Right]) -> Self {
        let mut capability = Self {
            read: false,
            write: false,
            admin: false,
            revoked: false,
        };
        for right in rights {
            match right {
                Right::Read => capability.read = true,
                Right::Write => capability.write = true,
                Right::Admin => capability.admin = true,
            }
        }
        capability
    }

    pub fn revoke(self) -> Self {
        Self {
            revoked: true,
            ..self
        }
    }

    pub fn is_revoked(&self) -> bool {
        self.revoked
    }

    fn holds(&self, right: Right) -> bool {
        match right {
            Right::Read => self.read,
            Right::Write => self.write,
            Right::Admin => self.admin,
        }
    }
}

pub fn authorizes(capability: &Capability, right: Right) -> bool {
    !capability.revoked && capability.holds(right)
}

pub fn revoke(capability: Capability) -> Capability {
    capability.revoke()
}
