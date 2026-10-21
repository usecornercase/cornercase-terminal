use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub(crate) struct TourArgs {
    #[command(subcommand)]
    pub(crate) action: Option<TourCommand>,
    #[arg(long, global = true, help = "Print JSON instead of text")]
    pub(crate) json: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TourCommand {
    #[command(about = "List the tours, their steps and how far you got")]
    List,
    #[command(about = "Start a tour in the window, where it stopped (the core tour by default)")]
    Start {
        #[arg(value_name = "TOUR", help = "A tour's id, from `cornercase tour list`")]
        tour: Option<String>,
        #[arg(long, value_name = "STEP", help = "Start at this step's id")]
        step: Option<String>,
        #[arg(long, help = "Keep the window's card hidden, for a program that draws its own")]
        no_card: bool,
    },
    #[command(about = "Mark the step on screen done and go to the next one")]
    Next,
    #[command(about = "Go back one step")]
    Back,
    #[command(about = "Go to the next step without marking this one done")]
    Skip,
    #[command(about = "Close the tour; its progress stays")]
    End,
    #[command(about = "Mark a step done")]
    Complete {
        #[arg(value_name = "TOUR")]
        tour: String,
        #[arg(value_name = "STEP")]
        step: String,
    },
    #[command(about = "Forget the progress of one tour, or of every tour")]
    Reset {
        #[arg(value_name = "TOUR")]
        tour: Option<String>,
    },
}
