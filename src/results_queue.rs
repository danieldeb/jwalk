use std::collections::BinaryHeap;
use std::sync::mpsc::{self, Receiver, SendError, Sender, TryRecvError};
use std::thread;

use crate::walk::{DirEntry, DirEntryContents};

#[derive(Clone)]
pub struct ResultsQueue {
  sender: Sender<DirEntryContents>,
}

pub struct ResultsQueueIterator {
  receiver: Receiver<DirEntryContents>,
}

pub struct SortedResultsQueueIterator {
  receiver: Receiver<DirEntryContents>,
  receive_buffer: BinaryHeap<DirEntryContents>,
  next_matcher: SortedResultsQueueNextMatcher,
}

struct SortedResultsQueueNextMatcher {
  index_path: Vec<usize>,
  remaining_siblings: Vec<usize>,
}

pub fn new_results_queue() -> (ResultsQueue, ResultsQueueIterator) {
  let (sender, receiver) = mpsc::channel();
  (ResultsQueue { sender }, ResultsQueueIterator { receiver })
}

pub fn new_sorted_results_queue() -> (ResultsQueue, SortedResultsQueueIterator) {
  let (sender, receiver) = mpsc::channel();
  (
    ResultsQueue { sender },
    SortedResultsQueueIterator {
      receiver,
      next_matcher: SortedResultsQueueNextMatcher::default(),
      receive_buffer: BinaryHeap::new(),
    },
  )
}

impl ResultsQueue {
  pub fn push(
    &self,
    dent: DirEntryContents,
  ) -> std::result::Result<(), SendError<DirEntryContents>> {
    self.sender.send(dent)
  }
}

impl Iterator for ResultsQueueIterator {
  type Item = DirEntryContents;
  fn next(&mut self) -> Option<DirEntryContents> {
    match self.receiver.recv() {
      Ok(entry) => Some(entry),
      Err(_) => None,
    }
  }
}

impl Iterator for SortedResultsQueueIterator {
  type Item = DirEntryContents;
  fn next(&mut self) -> Option<DirEntryContents> {
    while self.receive_buffer.peek().map(|i| &i.index_path) != Some(&self.next_matcher.index_path) {
      if self.next_matcher.is_none() {
        return None;
      }

      match self.receiver.try_recv() {
        Ok(dentry) => {
          self.receive_buffer.push(dentry);
          return self.receive_buffer.pop();
        }
        Err(err) => match err {
          TryRecvError::Empty => thread::yield_now(),
          TryRecvError::Disconnected => break,
        },
      }
    }

    if let Some(item) = self.receive_buffer.pop() {
      self.next_matcher.increment_past(&item);
      Some(item)
    } else {
      None
    }
  }
}

impl SortedResultsQueueNextMatcher {
  fn is_none(&self) -> bool {
    self.index_path.is_empty()
  }

  fn increment_past(&mut self, entry: &DirEntryContents) {
    // Decrement remaining siblings at this level
    *self.remaining_siblings.last_mut().unwrap() -= 1;

    if entry.remaining_folders_with_contents > 0 {
      // If visited item has children then push 0 index path, since we are now
      // looking for the first child.
      self.index_path.push(0);
      self
        .remaining_siblings
        .push(entry.remaining_folders_with_contents);
    } else {
      // Incrememnt sibling index
      *self.index_path.last_mut().unwrap() += 1;

      // If no siblings remain at this level unwind stacks
      while !self.remaining_siblings.is_empty() && *self.remaining_siblings.last().unwrap() == 0 {
        self.index_path.pop();
        self.remaining_siblings.pop();
        // Finished processing level, so increment sibling index
        if !self.index_path.is_empty() {
          *self.index_path.last_mut().unwrap() += 1;
        }
      }
    }
  }
}

impl Default for SortedResultsQueueNextMatcher {
  fn default() -> SortedResultsQueueNextMatcher {
    SortedResultsQueueNextMatcher {
      index_path: vec![0],
      remaining_siblings: vec![1],
    }
  }
}
