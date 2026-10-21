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
        let active = self.messages.tour.current().map(|(index, step, card)| {
            let tour = &TOURS[index];
            ActiveInfo { tour: tour.id.into(), step: tour.steps[step].id.into(), index: step, card }
        });
        TourState::of(self.messages.tour.saved(), active)
    }

    pub(super) fn tour_request(&mut self, request: &TourRequest) -> Handled {
        match request.action {
            TourAction::List => {}
            TourAction::Start => {
                let (index, found) = tour::find(request.tour.as_deref()).map_err(not_found)?;
                let step = request.step.as_deref().map(|s| tour::step(found, s)).transpose().map_err(not_found)?;
                self.start_tour(index, step, request.card.unwrap_or(true));
            }
            TourAction::Next | TourAction::Back | TourAction::Skip => self.move_tour_request(request)?,
            TourAction::End => self.end_tour(),
            TourAction::Complete => self.complete_tour_request(request)?,
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

    fn move_tour_request(&mut self, request: &TourRequest) -> Result<(), Refusal> {
        if self.messages.tour.current().is_none() {
            return Err(conflict("no tour is running; start one with `cornercase tour start`"));
        }
        if let Some(card) = request.card {
            self.messages.tour.show_card(card);
        }
        match request.action {
            TourAction::Next => self.tour_forward(),
            TourAction::Back => self.move_tour(Move::Back),
            _ => self.move_tour(Move::Skip),
        }
        Ok(())
    }

    fn complete_tour_request(&mut self, request: &TourRequest) -> Result<(), Refusal> {
        let (_, found) = tour::find(request.tour.as_deref()).map_err(not_found)?;
        let Some(step) = request.step.as_deref() else {
            return Err(Refusal::new(Code::Malformed, "`complete` needs a step"));
        };
        tour::step(found, step).map_err(not_found)?;
        self.complete_tour_step(found, step);
        Ok(())
    }
}
