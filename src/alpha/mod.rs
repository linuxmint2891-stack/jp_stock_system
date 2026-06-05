pub mod alpha_a;
pub mod alpha_b;
pub mod mean_reversion;
pub mod momentum;

pub enum Strategy {
    AlphaA,
    AlphaB,
    MeanReversion,
}
