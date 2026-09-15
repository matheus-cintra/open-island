use serde_json::Value;
use std::collections::HashMap;
use std::sync::mpsc::Sender;

pub type Reply = Result<Value, String>;
#[derive(Default)]
pub struct Pending {
    entries: HashMap<u64, (u64, Sender<Reply>)>,
}

impl Pending {
    pub fn insert(
        &mut self,
        id: u64,
        generation: u64,
        sender: Sender<Reply>,
    ) -> Result<(), String> {
        if self.entries.len() >= 64 {
            return Err("client_busy".to_owned());
        }
        self.entries.insert(id, (generation, sender));
        Ok(())
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.remove(&id);
    }

    pub fn settle(&mut self, id: u64, generation: u64, reply: Reply) {
        if self
            .entries
            .get(&id)
            .is_some_and(|(current, _)| *current == generation)
        {
            if let Some((_, sender)) = self.entries.remove(&id) {
                let _ = sender.send(reply);
            }
        }
    }

    pub fn disconnect(&mut self) {
        for (_, (_, sender)) in self.entries.drain() {
            let _ = sender.send(Err("daemon_unavailable".to_owned()));
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
