use super::{Handled, conflict, json};
use crate::app::App;
use crate::app::tour_card::Move;
use crate::control::contract::{Code, Refusal};
use crate::control::tour::{self, ActiveInfo, TourAction, TourAnswer, TourRequest, TourState};
use crate::log;
use crate::tour::TOURS;

fn not_found(message: String) -> Refusal {
    Refusal::new(Code::NotFound, message)
}

impl App {
    pub(in crate::app) fn tour_state(&self) -> TourState {
        let active = self.tour.active.as_ref().map(|a| {
            let tour = &TOURS[a.tour];
            ActiveInfo { tour: tour.id.into(), step: tour.steps[a.step].id.into(), index: a.step, card: a.card }
        });
        TourState::of(&self.tour.saved, active)
    }

    pub(super) fn tour_request(&mut self, request: &TourRequest) -> Handled {
        let running = || conflict("no tour is running; start one with `cornercase tour start`");
        match request.action {
            TourAction::List => {}
            TourAction::Start => {
                let (index, found) = tour::find(request.tour.as_deref()).map_err(not_found)?;
                let step = request.step.as_deref().map(|s| tour::step(found, s)).transpose().map_err(not_found)?;
                self.start_tour(index, step, request.card.unwrap_or(true));
            }
            TourAction::Next | TourAction::Back | TourAction::Skip => {
                if self.tour.active.is_none() {
                    return Err(running());
                }
                if let Some(card) = request.card
                    && let Some(active) = &mut self.tour.active
                {
                    active.card = card;
                }
                match request.action {
                    TourAction::Next => self.tour_forward(),
                    TourAction::Back => self.move_tour(Move::Back),
                    _ => self.move_tour(Move::Skip),
                }
            }
            TourAction::End => self.end_tour(),
            TourAction::Complete => {
                let (_, found) = tour::find(request.tour.as_deref()).map_err(not_found)?;
                let Some(step) = request.step.as_deref() else {
                    return Err(Refusal::new(Code::Malformed, "`complete` needs a step"));
                };
                tour::step(found, step).map_err(not_found)?;
                self.complete_tour_step(found, step);
            }
            TourAction::Reset => {
                if let Some(id) = request.tour.as_deref() {
                    tour::find(Some(id)).map_err(not_found)?;
                }
                self.reset_tour(request.tour.as_deref());
                log::info!("tour", "reset", tour = request.tour.as_deref().unwrap_or("all"));
            }
        }
        let tours = if request.action == TourAction::List { tour::tours() } else { Vec::new() };
        Ok(Some(json(&TourAnswer { state: self.tour_state(), tours })))
    }
}
