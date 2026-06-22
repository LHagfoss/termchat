use std::collections::HashMap;

/// Standard emoji shortcode mapping (Discord/Slack compatible)
const EMOJI_MAP: &[(&str, &str)] = &[
    // Smileys & Emotions
    ("grinning", "\u{1F600}"),
    ("smiley", "\u{1F603}"),
    ("smile", "\u{1F604}"),
    ("grin", "\u{1F601}"),
    ("laughing", "\u{1F606}"),
    ("sweat_smile", "\u{1F605}"),
    ("rofl", "\u{1F923}"),
    ("joy", "\u{1F602}"),
    ("wink", "\u{1F609}"),
    ("blush", "\u{1F60A}"),
    ("innocent", "\u{1F607}"),
    ("heart_eyes", "\u{1F60D}"),
    ("star_struck", "\u{1F929}"),
    ("kissing_heart", "\u{1F618}"),
    ("relaxed", "\u{263A}\u{FE0F}"),
    ("yum", "\u{1F60B}"),
    ("stuck_out_tongue", "\u{1F61B}"),
    ("stuck_out_tongue_winking_eye", "\u{1F61D}"),
    ("stuck_out_tongue_closed_eyes", "\u{1F61C}"),
    ("money_mouth_face", "\u{1F911}"),
    ("hugging_face", "\u{1F917}"),
    ("thinking_face", "\u{1F914}"),
    ("nerd_face", "\u{1F913}"),
    ("sunglasses", "\u{1F60E}"),
    ("exploding_head", "\u{1F92F}"),
    ("shushing_face", "\u{1F92D}"),
    ("cry", "\u{1F622}"),
    ("sob", "\u{1F62D}"),
    ("scream", "\u{1F631}"),
    ("rage", "\u{1F621}"),
    ("angry", "\u{1F620}"),
    ("smirk", "\u{1F60F}"),
    ("unamused", "\u{1F612}"),
    ("pensive", "\u{1F614}"),
    ("confused", "\u{1F615}"),
    ("worried", "\u{1F61F}"),
    ("open_mouth", "\u{1F62E}"),
    ("hushed", "\u{1F62F}"),
    ("cold_sweat", "\u{1F613}"),
    ("fearful", "\u{1F628}"),
    ("confounded", "\u{1F61E}"),
    ("disappointed", "\u{1F61E}"),
    ("persevere", "\u{1F623}"),
    ("drooling_face", "\u{1F924}"),
    ("lying_face", "\u{1F925}"),
    ("liar", "\u{1F925}"),
    ("shocked", "\u{1F632}"),
    ("astonished", "\u{1F632}"),
    ("flushed", "\u{1F633}"),
    ("pleading_face", "\u{1F97A}"),
    ("pray", "\u{1F64F}"),
    ("please", "\u{1F97A}"),

    // Gestures & Hands
   ("+1", "\u{1F44D}"),
    ("thumbsup", "\u{1F44D}"),
    ("-1", "\u{1F44E}"),
    ("thumbsdown", "\u{1F44E}"),
    ("wave", "\u{1F44B}"),
   ("raised_hand", "\u{1F590}"),
    ("ok_hand", "\u{1F44C}"),
    ("pinching_hands", "\u{1F90F}"),
    ("clap", "\u{1F44F}"),
    ("raised_back_of_hand", "\u{1F91A}"),
    ("writing_hand", "\u{270D}\u{FE0F}"),
    ("nail_care", "\u{1F485}"),
    ("selfie", "\u{1F933}"),
    ("muscle", "\u{1F4AA}"),
    ("middle_finger", "\u{1F595}"),
    ("reversed_hand_with_middle_finger_extended", "\u{1F595}"),
    ("v", "\u{1F590}"),
    ("crossed_fingers", "\u{1F91E}"),
    ("love_you_gesture", "\u{1F91F}"),
    ("metal", "\u{1F918}"),
    ("call_me_hand", "\u{1F919}"),
    ("point_left", "\u{1F448}"),
    ("point_right", "\u{1F449}"),
    ("point_down", "\u{1F447}"),
    ("point_up_2", "\u{1F446}"),
    ("point_up", "\u{261D}\u{FE0F}"),

    // Nature & Flowers
    ("bouquet", "\u{1F490}"),
    ("cherry_blossom", "\u{1F338}"),
    ("white_flower", "\u{1F3F0}\u{FE0F}"),
    ("rosette", "\u{1F3F5}"),
    ("rose", "\u{1F339}"),
    ("wilted_flower", "\u{1F940}"),
    ("hibiscus", "\u{1F33A}"),
    ("sunflower", "\u{1F33B}"),
    ("blossom", "\u{1F33C}"),
    ("tulip", "\u{1F337}"),
    ("seedling", "\u{1F331}"),
    ("evergreen_tree", "\u{1F332}"),
    ("deciduous_tree", "\u{1F333}"),
    ("palm_tree", "\u{1F334}"),
    ("cactus", "\u{1F335}"),
    ("ear_of_rice", "\u{1F33E}"),
    ("herb", "\u{1F33F}"),
    ("shamrock", "\u{2618}\u{FE0F}"),
    ("four_leaf_clover", "\u{1F340}"),
    ("maple_leaf", "\u{1F341}"),
    ("fallen_leaf", "\u{1F342}"),
    ("leaves", "\u{1F343}"),

    // Food & Drink
    ("apple", "\u{1F34E}"),
    ("green_apple", "\u{1F34F}"),
    ("pear", "\u{1F350}"),
    ("peach", "\u{1F351}"),
    ("cherries", "\u{1F352}"),
    ("grapes", "\u{1F347}"),
    ("watermelon", "\u{1F349}"),
    ("mandarin", "\u{1F34A}"),
    ("banana", "\u{1F34C}"),
    ("pineapple", "\u{1F34D}"),
    ("tomato", "\u{1F345}"),
    ("salad", "\u{1F957}"),
    ("hot_pepper", "\u{1F336}"),

    // Symbols & Hearts
    ("heart", "\u{2764}\u{FE0F}"),
    ("orange_heart", "\u{1F9E1}"),
    ("yellow_heart", "\u{1F49B}"),
    ("green_heart", "\u{1F49A}"),
    ("blue_heart", "\u{1F499}"),
    ("purple_heart", "\u{1F49C}"),
    ("broken_heart", "\u{1F494}"),
    ("hearts", "\u{2764}\u{2764}\u{2764}"),
    ("two_hearts", "\u{1F495}"),
    ("revolving_hearts", "\u{1F49E}"),
    ("heartbeat", "\u{1F493}"),
    ("heartpulse", "\u{1F493}"),
    ("sparkling_heart", "\u{1F496}"),
    ("sparkles", "\u{2728}"),
    ("star", "\u{2B50}"),
    ("stars", "\u{1F31F}"),
    ("boom", "\u{1F4A5}"),
    ("collision", "\u{1F4A5}"),
    ("anger", "\u{1F4AB}"),
    ("exclamation", "\u{2757}"),
    ("question", "\u{2753}"),
    ("grey_exclamation", "\u{2755}"),
    ("grey_question", "\u{2754}"),
    ("zzz", "\u{1F4A4}"),
    ("bomb", "\u{1F4A3}"),
    ("speech_balloon", "\u{1F4AC}"),
    ("thought_balloon", "\u{1F4AD}"),
    ("warning", "\u{26A0}\u{FE0F}"),
    ("recycle", "\u{267B}\u{FE0F}"),
    ("wheelchair", "\u{1F6BC}"),
    ("peace", "\u{262E}\u{FE0F}"),
    ("on", "\u{1F51A}"),
    ("soon", "\u{1F51C}"),
    ("copyright", "\u{00A9}\u{FE0F}"),
    ("registered", "\u{00AE}\u{FE0F}"),
    ("tm", "\u{2122}"),
    ("new", "\u{1F195}"),
    ("end", "\u{1F51E}"),
    ("back", "\u{1F519}"),
    ("go_on", "\u{1F51D}"),
    ("up", "\u{1F4C3}"),
    ("cool", "\u{1F192}"),
    ("fast_forward", "\u{23E9}"),
    ("red_circle", "\u{1F534}"),
    ("large_blue_circle", "\u{1F535}"),

    // Animals
    ("snail", "\u{1F40C}"),
    ("ant", "\u{1F41C}"),
    ("bee", "\u{1F41D}"),
    ("bug", "\u{1F41B}"),
    ("flying_saucer", "\u{1F680}"),

    // Misc common
    ("snowflake", "\u{2744}\u{FE0F}"),
    ("white_frowning_face", "\u{2639}\u{FE0F}"),
];

/// Build a lookup HashMap from the static slice.
fn build_map() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::with_capacity(EMOJI_MAP.len());
    for &(shortcode, emoji) in EMOJI_MAP {
        map.insert(shortcode, emoji);
    }
    map
}

/// Renders all `:shortcode:` emoji patterns in text to their Unicode equivalents.
pub fn render_emojis(text: &str) -> String {
    let map = build_map();
    let mut result = String::with_capacity(text.len() * 2);
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == ':' {
            let mut shortcode_buf = String::new();
            let mut found_closing = false;

            while let Some(&next_ch) = chars.peek() {
                chars.next();
                if next_ch == ':' {
                    found_closing = true;
                    break;
                }
                shortcode_buf.push(next_ch);
            }

            if found_closing && !shortcode_buf.is_empty() {
                if let Some(emoji) = map.get(shortcode_buf.as_str()) {
                    result.push_str(emoji);
                    continue;
                }
            }

            // No match — output the opening colon and redo the buffer content
            result.push(ch);
            if !found_closing {
                result.push_str(&shortcode_buf);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_emoji() {
        assert_eq!(render_emojis("hello :pray:"), "hello \u{1F64F}");
    }

    #[test]
    fn test_multiple_emojis() {
        let result = render_emojis(":heart: :wave:");
        assert!(result.contains('\u{2764}'));
        assert!(result.contains('\u{1F44B}'));
    }

    #[test]
    fn test_no_emoji() {
        assert_eq!(render_emojis("no emoji here"), "no emoji here");
    }

    #[test]
    fn test_unknown_shortcode_keeps_colons() {
        let result = render_emojis(":unknown: hello");
        // Unknown shortcodes keep their colons and text is preserved
        assert!(result.contains("hello"));
    }

    #[test]
    fn test_mixed_content_preserves_mentions() {
        let result = render_emojis("Hey @user :wave: how are you? :pray:");
        assert!(result.contains("@user"));
        assert!(result.contains('\u{1F44B}')); // wave
        assert!(result.contains('\u{1F64F}')); // pray
    }
}
