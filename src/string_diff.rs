//! Used for finding the minimal set of operations to transform one string into another.
//!
//! The primary function of this module is [find diff](fn.find_diff.html).
use std::mem;
use std::cmp::max;
use super::{Diff};


/// Finds the difference on a character by character level between two strings
///
/// Uses the Hirschberg algorithm (doi: [10.1145/360825.360861](http://dx.doi.org/10.1145/360825.360861))
/// which operates in `O(x * y)` time and `O(y)` space.  The algorithm finds the minimal set of operations
/// that will transform 'old' into 'new'.  The 'weight' of each operation is determined by the `scorer.`
/// For more details about weighting, see the [OperationScore](trait.OperationScore.html) documentation.
///
/// The operations in the returned `Diff `are presented in file order, with offsets assuming the
/// previous operations have already been performed.  Furthermore, the inserts are assumed to
/// be performed prior to the deletes.
///
/// # Example
///
/// ```
/// use rdiff::string_diff::{find_diff, EditDistance};
/// // Find the difference between meadow and yellowing using the edit distance as the weighting.
/// let diff = find_diff("meadow", "yellowing", &EditDistance{});
/// // prints (0, 'y'), (3, 'll') and (9, 'ing')
/// for insert in diff.inserts() {
///     println!("{:?}", insert);
/// }
/// // prints (1, 1) and (4, 2)
/// for delete in diff.deletes() {
///     println!("{:?}", delete);
/// }
/// assert_eq!("yellowing", diff.apply_to_string("meadow").unwrap());
/// ```
pub fn find_diff<S: OperationScore>(old: &str, new: &str, scorer: &S) -> Diff {
    let mut diff = Diff::new();
    let mut insert_index = 0;
    let mut delete_index = 0;
    let old_rev = old.chars().rev().collect::<String>();
    let new_rev = new.chars().rev().collect::<String>();
    hirschberg(old, new, &old_rev, &new_rev, scorer, &mut diff, &mut insert_index, &mut delete_index);
    diff
}

/// Handles updating the diff and relevant indexes when inserting a string
/// Needed because the string must be converted to bytes before it can be used in the diff
macro_rules! do_insert {
    ($s: expr, $index: expr, $diff: expr) => (
        {
            let bytes = $s.bytes().collect::<Vec<_> >();
            let byte_len = bytes.len();
            $diff.add_insert(*$index, bytes);
            *$index += byte_len;
        }
    );
}

/// Handles updating the diff and relevant indexes when deleting a suvstring
/// Needed because the string must be converted to bytes before it can be used in the diff
macro_rules! do_delete {
    ($length: expr, $delete_index: expr, $insert_index: expr, $diff: expr) => (
        {
            $diff.add_delete(*$insert_index - *$delete_index, $length);
            *$delete_index += $length;
            *$insert_index += $length;
        }
    );
}

/// Uses the Hirschberg algorithm to calculate the optimal set of operations to transform 'old' into 'new'.
/// The only parameters that are input are 'old', 'new' and `scorer`.  `x_rev` and `y_rev` are just
/// cached so that 'old' and 'new' don't need to be reversed for every recursion of the algorithm.
/// `diff` is the output of the algorithm and `insert_index` and `delete_index` are simply intermediate state
/// being passed around.
fn hirschberg<S: OperationScore>(old: &str, new: &str, old_rev: &str, new_rev: &str, scorer: &S, diff: &mut Diff, insert_index: &mut usize, delete_index: &mut usize) {
    trace!("'{}' ({}) '{}' ({})", old, old_rev, new, new_rev);
    // We're going to use these lengths over and over again, we might as well cache them.
    let old_len = old.len();
    let new_len = new.len();

    // If one of the two strings is 0, then it's trvial to transform one into the other
    if old_len == 0 {
        do_insert!(new, insert_index, diff);
    } else if new_len == 0 {
        do_delete!(old_len, delete_index, insert_index, diff);
    }
    // If old is legnth 1, then there are two cases:
    else if old_len == 1 {
        let old_char = old.chars().next().unwrap();
        match new.chars().position(|c| c == old_char) {
            // Either new contains old, in which case
            Some(position) => {
                // We insert whatever is on the left of old in new
                if position > 0 {
                    do_insert!(new[..position], insert_index, diff);
                }
                *insert_index += 1;
                // and we insert whatever is on the right of old in new
                if new_len - position > 1 {
                    do_insert!(new[position + 1..], insert_index, diff);
                }
            } None => {
                //or new does not contain old, in which case
                // we simply delete old and insert new
                do_insert!(new, insert_index, diff);
                do_delete!(1, delete_index, insert_index, diff);
            }
        }
    }
    // If new is length 1, then there are two cases:
    else if new_len == 1 {
        let new_char = new.chars().next().unwrap();
        match old.chars().position(|c| c == new_char) {
            // either old contains new, in which case
            Some(position) => {
                // We delete everything in old to the left of new
                if position > 0 {
                    do_delete!(position, delete_index, insert_index, diff);
                }
                *insert_index += 1;
                // and we delete everything in old to the right of new
                if old_len - position > 1 {
                    let delete_len = old_len - position - 1;
                    do_delete!(delete_len, delete_index, insert_index, diff);
                }
            } None => {
                // or old does not contain new, in which case we simply insert new and delete
                // everything that was previously in old
                do_insert!(new, insert_index, diff);
                do_delete!(old_len, delete_index, insert_index, diff);
            }
        }
    } else {
        // If it's not trivial, then we recurse until it is.
        // We begin bnew dividing old in half.
        let old_mid = old_len / 2;
        // We then find the index in new where splitting the string will give us the
        // highest possible score.  This index is the point where the trace of the edit
        // operations performed is guaranteed to cross.
        let score_l = nw_score(&old[..old_mid], new, scorer);
        let score_r = nw_score(&old_rev[..old_len - old_mid], new_rev, scorer);
        let new_mid = score_l.iter()
                            .zip(score_r.iter().rev())
                            .map(|(l, r)| l + r)
                            .zip(0..new_len + 1).max().unwrap().1;
        // We then recurse on the left side of old and new
        hirschberg(&old[..old_mid], &new[..new_mid], &old_rev[old_len - old_mid..], &new_rev[new_len - new_mid..], scorer, diff, insert_index, delete_index);
        // and the right side of old and new
        hirschberg(&old[old_mid..], &new[new_mid..], &old_rev[..old_len - old_mid], &new_rev[..new_len - new_mid], scorer, diff, insert_index, delete_index);


    }

}

/// Used to calculate the score for each operation that
/// will be performed.  The score can be static, or it can
/// vary based on which character is being deleted inserted or substituted.
/// It is highly recommended to inline the implementation of these characters
pub trait OperationScore {
    /// The score for inserting character `c` into the string
    fn insert_score(&self, c: char) -> i32;
    /// The score for deleting character `c` from the string
    fn delete_score(&self, c: char) -> i32;
    /// The score for replacing character `old` with character `new`
    fn substitution_score(&self, old: char, new: char) -> i32;
    /// The score for when a character is one string matches the character in the other string
    fn match_score(&self, c: char) -> i32;
}

/// Used as the classiscal definition of edit distance.
///
/// That is:
///
/// * Insert is cost -1
/// * Delete is cost -1
/// * Substitution is cost -2 (an insert + a delete)
/// * Matching is cost 0
pub struct EditDistance;

impl OperationScore for EditDistance {
    #[inline]
    fn insert_score(&self, _: char) -> i32 {
        -1
    }

    #[inline]
    fn delete_score(&self, _: char) -> i32 {
        -1
    }

    #[inline]
    fn substitution_score(&self, _: char, _: char) -> i32 {
        -2
    }

    #[inline]
    fn match_score(&self, _: char) -> i32 {
        0
    }
}

/// Calculate the score based on the Needleman-Wunsch algorithm.  This algorithm
/// calculates the cost of transforming string 'old' into string 'new' using operation scoring
/// given by `scorer`.
///
/// It operates by iteratively generating the score for progressively longer
/// substrings of 'old' and 'new'.  The result is a vector of the transformation score
/// from 'old' to a substring of length `i` of 'new' where `i` is the index of an element in
/// the resulting vector.
fn nw_score<S: OperationScore>(old: &str, new: &str, scorer: &S) -> Vec<i32> {

    trace!("nw_score for '{}' - '{}'", old, new);
    let row_len = new.len() + 1;
    let mut last_row = Vec::with_capacity(row_len);
    let mut this_row = Vec::with_capacity(row_len);
    let mut total_insert = 0;
    last_row.push(0);
    for new_char in new.chars() {
        total_insert += scorer.insert_score(new_char);
        last_row.push(total_insert);
    }
    trace!("{:?}", last_row);
    for old_char in old.chars() {
        this_row.push(last_row[0] + scorer.delete_score(old_char));
        for (new_index, new_char) in new.chars().enumerate() {
            let score_sub = last_row[new_index] + if old_char == new_char {
                scorer.match_score(old_char)
            } else {
                scorer.substitution_score(old_char, new_char)
            };
            let score_del = last_row[new_index + 1] + scorer.delete_score(old_char);
            let score_ins = this_row[new_index] + scorer.insert_score(new_char);
            this_row.push(max(max(score_sub, score_del), score_ins))
        }
        trace!("{:?}", this_row);
        last_row = mem::replace(&mut this_row, Vec::with_capacity(row_len));
    }
    last_row

}

#[cfg(test)]
mod test {
    extern crate env_logger;
    use super::{nw_score, find_diff, EditDistance, OperationScore};
    use super::super::{Insert, Delete, Diff};

    struct ExampleScores;

    macro_rules! check_diff {
        ($start: tt |  $new: tt | $scorer: tt | $(($insert_pos : tt, $insert_value: tt)),* | $(($delete_pos: tt, $delete_len: tt)),*) => {
            {
                check_diff_workaround!($start; $new; $scorer; $(($insert_pos, $insert_value)),*; $(($delete_pos, $delete_len)),*)
            }
        };
    }

    // Caused by a bug in the implementation of the tt macro type.  It currently has to be passed as an expr into another macro
    // or it throws a fit for no reason.  See https://github.com/rust-lang/rust/issues/5846
    macro_rules! check_diff_workaround {
        ($start: expr ; $new: expr ; $scorer: expr; $(($insert_pos : tt, $insert_value: tt)),* ; $(($delete_pos: tt, $delete_len: tt)),*) => {
            {
                let diff = find_diff($start, $new, &$scorer);
                assert_eq!(Diff {
                    inserts: vec![$(Insert{position: $insert_pos, data: $insert_value.bytes().collect()}),*],
                    deletes: vec![$(Delete{position: $delete_pos, len: $delete_len}),*]
                }, diff);
                assert_eq!(diff.apply_to_string($start).unwrap(), $new.to_string());
            }
        };
    }

    // From the wikipedia example at https://en.wikipedia.org/wiki/Hirschberg%27s_algorithm
    impl OperationScore for ExampleScores {
        #[inline]
        fn insert_score(&self, _: char) -> i32 {
            -2
        }

        #[inline]
        fn delete_score(&self, _: char) -> i32 {
            -2
        }

        #[inline]
        fn substitution_score(&self, _: char, _: char) -> i32 {
            -1
        }

        #[inline]
        fn match_score(&self, _: char) -> i32 {
            2
        }
    }

    #[test]
    fn score() {
        assert_eq!(nw_score("ACGC", "CGTAT", &EditDistance{}), vec![-4, -3, -2, -3, -4, -5]);
        assert_eq!(nw_score("AGTA", "TATGC", &EditDistance{}), vec![-4, -3, -2, -3, -4, -5]);

        assert_eq!(nw_score("ACGC", "CGTAT", &ExampleScores{}), vec![-8, -4, 0, 1, -1, -3]);
        assert_eq!(nw_score("AGTA", "TATGC", &ExampleScores{}), vec![-8, -4, 0, -2, -1, -3]);
    }

    #[test]
    fn do_find_diff() {
        //env_logger::init().unwrap();
        check_diff!(
            "kitten" |
            "kettle" |
            EditDistance |
            (1, "e"), (5, "l") |
            (2, 1), (6, 1)
        );
        check_diff!(
            "meadow" |
            "yellowing" |
            EditDistance |
            (0, "y"), (3, "ll"), (9, "ing") |
            (1, 1), (4, 2)
        );

        check_diff!(" I've" |
                    " I" |
                    EditDistance |
                    |
                    (2, 3)
                );

        check_diff!(" I've got a new place" |
                    " I found a new place" |
                    EditDistance |
                    (6, "f"), (9, "und") |
                    (2, 3), (4, 1), (8, 1)
                );
        check_diff!(
            "Since my baby left me I've got a new place to dwell\nI walk down a lonely street to Heartbreak Hotel." |
            "Since my baby left me I found a new place to dwell\nDown at the end of 'Lonely Street' to 'Heartbreak Hotel.'" |
            EditDistance |
            (27, "f"), (30, "und"), (56, "Down"), (64, "t the"), (72, "en"), (75, " "), (77, "f"), (81, "'L"), (92, "S"), (99, "'"),  (104, "'"), (122, "'") |
            (23, 3), (25, 1), (29, 1),(55, 1), (56, 1), (62, 2), (69, 2), (72, 3), (79, 1)
        );
    }
}
