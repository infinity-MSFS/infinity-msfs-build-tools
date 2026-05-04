use crate::{Artifact, BuildResult};

pub trait Builder {
    type Input;
    type Output: Artifact;

    fn build(&self, input: &Self::Input) -> BuildResult<Self::Output>;
}
