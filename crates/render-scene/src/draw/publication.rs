use std::{cell::RefCell, rc::Rc};

use super::visibility::Selection;

pub(crate) struct SelectionPublication {
    pub(super) destination: Rc<RefCell<Selection>>,
    pub(super) selection: Selection,
}

/// Draw results remain local until the scheduler checks its encoder after rendering.
#[derive(Default)]
pub(crate) struct DrawPublications(RefCell<Vec<SelectionPublication>>);

impl DrawPublications {
    pub fn record(&self, selection: SelectionPublication) {
        self.0.borrow_mut().push(selection);
    }

    pub fn publish(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        for SelectionPublication {
            destination,
            selection,
        } in self.0.get_mut().drain(..)
        {
            crate::compositor::publication::stage(context, &destination.clone(), move || {
                *destination.borrow_mut() = selection;
            });
        }
    }
}
