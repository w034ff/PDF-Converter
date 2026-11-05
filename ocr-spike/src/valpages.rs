//! Vertical item 1: the validation pages — generated vertical pages kept
//! apart from the four development pages of item 5 (`vertical.rs`).
//!
//! Other public-domain prose (Dazai, Mori Ōgai, Terada Torahiko, Kajii,
//! Miyazawa's "Gauche", Higuchi Ichiyō), typed in because Aozora Bunko is not
//! reachable from the sandbox; the ground truth is what is drawn, so small
//! differences from the printed editions do not matter. Five pages are set
//! in Noto Serif JP (Mincho), two in IBM Plex Sans JP; heading sizes and
//! tiers differ from the development pages (4 tiers, 120 px and 96 px titles,
//! a dense 28 px page, a 46 px page). Glyphs are placed with their em box at
//! the pen (`Typesetter::em_align`), so `char_boxes` are exact.

use crate::vertical::{GtBlock, PAGE_H, PAGE_W, Typesetter, hline, vblock};

pub const MEROSU: &str = "メロスは激怒した。必ず、かの邪智暴虐の王を除かなければならぬと決意した。メロスには政治がわからぬ。メロスは、村の牧人である。笛を吹き、羊と遊んで暮して来た。けれども邪悪に対しては、人一倍に敏感であった。きょう未明メロスは村を出発し、野を越え山越え、十里はなれた此のシラクスの市にやって来た。メロスには父も、母も無い。女房も無い。十六の、内気な妹と二人暮しだ。この妹は、村の或る律気な一牧人を、近々、花婿として迎える事になっていた。結婚式も間近かなのである。メロスは、それゆえ、花嫁の衣裳やら祝宴の御馳走やらを買いに、はるばる市にやって来たのだ。先ず、その品々を買い集め、それから都の大路をぶらぶら歩いた。メロスには竹馬の友があった。セリヌンティウスである。今は此のシラクスの市で、石工をしている。その友を、これから訪ねてみるつもりなのだ。久しく逢わなかったのだから、訪ねて行くのが楽しみである。歩いているうちにメロスは、まちの様子を怪しく思った。ひっそりしている。もう既に日も落ちて、まちの暗いのは当りまえだが、けれども、なんだか、夜のせいばかりでは無く、市全体が、やけに寂しい。のんきなメロスも、だんだん不安になって来た。路で逢った若い衆をつかまえて、何かあったのか、二年まえに此の市に来たときは、夜でも皆が歌をうたって、まちは賑やかであった筈だが、と質問した。若い衆は、首を振って答えなかった。しばらく歩いて老爺に逢い、こんどはもっと、語勢を強くして質問した。老爺は答えなかった。メロスは両手で老爺のからだをゆすぶって質問を重ねた。老爺は、あたりをはばかる低声で、わずか答えた。「王様は、人を殺します。」「なぜ殺すのだ。」「悪心を抱いている、というのですが、誰もそんな、悪心を持っては居りませぬ。」「たくさんの人を殺したのか。」「はい、はじめは王様の妹婿さまを。それから、御自身のお世嗣を。それから、妹さまを。それから、妹さまの御子さまを。それから、皇后さまを。それから、賢臣のアレキス様を。」「おどろいた。国王は乱心か。」「いいえ、乱心ではございませぬ。人を、信ずる事が出来ぬ、というのです。」";
pub const TAKASEBUNE: &str = "高瀬舟は京都の高瀬川を上下する小舟である。徳川時代に京都の罪人が遠島を申し渡されると、本人の親類が牢屋敷へ呼び出されて、そこで暇乞いをすることを許された。それから罪人は高瀬舟に載せられて、大阪へ回されることであった。それを護送するのは、京都町奉行の配下にいる同心で、この同心は罪人の親類の中で、主立った一人を大阪まで同船させることを許す慣例であった。これは上へ通った事ではないが、いわゆる大目に見るのであった、黙許であった。庄兵衛はとうとうこらえ切れなくなって呼び掛けた。「喜助。お前何を思っているのか。」「はい」と云ってあたりを見廻した喜助は、何事をかお役人に見咎められたのではないかと気遣うらしく、居ずまいを直して庄兵衛の気色を伺った。「いや。別にわけがあって聞いたのではない。実はな、己は先刻からお前の島へ往く心持が聞いてみたかったのだ。己はこれまでこの舟で大勢の人を島へ送った。それは随分いろいろな身の上の人だったが、どれもどれも島へ往くのを悲しがって、見送りに来て、一しょに舟に乗る親類のものと、夜どおし泣くに極まっていた。」喜助はにっこり笑った。「御親切に仰ゃって下すって、難有うございます。」";
pub const COFFEE: &str = "八九歳のころ医者の命令で始めて牛乳を飲まされた。当時まだ牛乳は少なくとも大衆一般の嗜好品でもなく、常用栄養品でもなく、主として病弱な人間の薬用品であったように見える。そうして、牛乳やいわゆるソップがどうにも臭くって飲めず、飲めばきっと嘔吐したりしたような古風な趣味の人の多かった時代であった。その頃の牛乳は壜ではなくてブリキ鑵に入れて配達されたものである。この牛乳の口直しに医者はコーヒーを少しまぜることを教えてくれた。粉にしたコーヒーをさらし木綿の小袋にほんの一つまみちょっぴり入れたのを熱い牛乳の中に浸して、漢方の風邪薬のように振り出し絞り出すのである。とにかくこの生まれて始めて味わったコーヒーの香味はすっかり田舎育ちの少年の私を心酔させてしまった。すべてのエキゾチックなものに憧憬をもっていた子供心に、この南洋的西洋的な香気は未知の極楽郷から遠洋を渡って来た一脈の薫風のように感ぜられたもののようである。";
pub const LEMON: &str = "えたいの知れない不吉な塊が私の心を始終圧えつけていた。焦躁と言おうか、嫌悪と言おうか――酒を飲んだあとに宿酔があるように、酒を毎日飲んでいると宿酔に相当した時期がやって来る。それが来たのだ。これはちょっといけなかった。結果した肺尖カタルや神経衰弱がいけないのではない。また背を焼くような借金などがいけないのではない。いけないのはその不吉な塊だ。以前私を喜ばせたどんな美しい音楽も、どんな美しい詩の一節も辛抱がならなくなった。蓄音器を聴かせてもらいにわざわざ出かけて行っても、最初の二三小節で不意に立ち上がってしまいたくなる。何かが私を居堪らずさせるのだ。それで始終私は街から街を浮浪し続けていた。生活がまだ蝕まれていなかった以前私の好きであった所は、たとえば丸善であった。赤や黄のオードコロンやオードキニン。洒落た切子細工や典雅なロココ趣味の浮模様を持った琥珀色や翡翠色の香水壜。煙管、小刀、石鹸、煙草。私はそんなものを見るのに小一時間も費すことがあった。そして結局一等いい鉛筆を一本買うくらいの贅沢をするのだった。しかしここももうその頃の私にとっては重くるしい場所に過ぎなかった。書籍、学生、勘定台、これらはみな借金取りの亡霊のように私には見えるのだった。";
pub const GAUCHE: &str = "ゴーシュは町の活動写真館でセロを弾く係りでした。けれどもあんまり上手でないという評判でした。上手でないどころではなく実は仲間の楽手のなかではいちばん下手でしたから、いつでも楽長にいじめられるのでした。ひるすぎみんなは楽屋に円くならんで今度の町の音楽会へ出す第六交響曲の練習をしていました。トランペットは一生けん命歌っています。ヴァイオリンも二いろ風のように鳴っています。クラリネットもボーボーとそれに手伝っています。ゴーシュも口をりんと結んで眼を皿のようにして楽譜を見つめながらもう一心に弾いています。にわかにぱたっと楽長が両手を鳴らしました。みんなぴたりと曲をやめてしんとしました。楽長がどなりました。「セロがおくれた。トォテテテテテイ、ここからやり直し。はいっ。」みんなは今の所の少し前の所からやり直しました。ゴーシュは顔をまっ赤にして額に汗を出しながらやっといま云われたところを通りました。ほっと安心しながら、つづけて弾いていますと楽長がまた手をぱっと拍ちました。「セロっ。糸が合わない。困るなあ。ぼくはきみにドレミファを教えてまでいるひまはないんだがなあ。」";
pub const SHAYOU: &str = "朝、食堂でスウプを一さじ、すっと吸ってお母さまが、「あ。」と幽かな叫び声をお挙げになった。「髪の毛？」スウプに何か、イヤなものでも入っていたのかしら、と思った。「いいえ。」お母さまは、何事も無かったように、またひらりと一さじ、スウプをお口に流し込み、すましてお顔を横に向け、お勝手の窓の、満開の山桜に視線を送り、そうしてお顔を横に向けたまま、またひらりと一さじ、スウプを小さなお唇のあいだに滑り込ませた。ひらりという形容は、お母さまの場合、決して誇張では無い。婦人雑誌などに出ているお食事のいただき方などとは、てんでまるで、違っていらっしゃる。弟の直治がいつか、お酒を飲みながら、姉の私に向ってこう言った事がある。「爵位があるから、貴族だというわけにはいかないんだぜ。爵位が無くても、天爵というものを持っている立派な貴族のひともあるし、おれたちのように爵位だけは持っていても、貴族どころか、賤民にちかいのもいる。」";
pub const TAKEKURABE: &str = "廻れば大門の見返り柳いと長けれど、お歯ぐろ溝に燈火うつる三階の騒ぎも手に取る如く、明けくれなしの車の行来にはかり知られぬ全盛をうらなひて、大音寺前と名は仏くさけれど、さりとは陽気の町と住みたる人の申き、三島神社の角をまがりてより是れぞと見ゆる大厦もなく、かたぶく軒端の十軒長屋二十軒長や、商ひはかつふつ利かぬ処とて半さしたる雨戸の外に、あやしき形に紙を切りなして、胡粉ぬりくり彩色のある田楽みるやう、裏にはりたる串のさまもをかし。一軒ならず二軒ならず、朝日に干して夕日に仕舞ふ手当ことごとしく、一家内これにかかりて夫れは何ぞと問ふに、知らずや霜月酉の日例の神社に欲深様のかつぎ給ふ是れぞ熊手の下ごしらへといふ。";

/// The text that fits in `cols` columns of `per` characters.
fn fit(text: &str, cols: usize, per: usize) -> String {
    text.chars().take(cols * per).collect()
}

/// Columns of `pitch` that fit between `right` and the left margin.
fn cols_between(right: f32, size: f32, pitch: f32, left_margin: f32) -> usize {
    ((right - size - left_margin) / pitch).floor() as usize + 1
}

/// Tiers of `per`-character columns, `cols` columns each, `gap` between tiers.
#[allow(clippy::too_many_arguments)]
fn tiers(ts: &Typesetter, img: &mut image::GrayImage, text: &str, size: f32, pitch: f32, right: f32, top: f32, per: usize, cols: usize, gap: f32) -> Vec<GtBlock> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(per * cols)
        .enumerate()
        .map(|(t, part)| {
            let s: String = part.iter().collect();
            vblock(ts, img, &s, size, pitch, right, top + t as f32 * (per as f32 * size + gap), per)
        })
        .collect()
}

/// The validation pages: (name, image, blocks in reading order).
/// `mincho` and `gothic` must have `em_align` set.
pub fn pages(mincho: &Typesetter, gothic: &Typesetter) -> Vec<(String, image::GrayImage, Vec<GtBlock>)> {
    let blank = || image::GrayImage::from_pixel(PAGE_W, PAGE_H, image::Luma([255]));
    let (r, margin) = (PAGE_W as f32 - 130.0, 130.0);
    let mut out = Vec::new();

    // t1 (Mincho): 96 px title, author at the foot of the next column, one body block of 54-character columns.
    let mut img = blank();
    let mut b = vec![vblock(mincho, &mut img, "走れメロス", 96.0, 120.0, r, 200.0, 5)];
    b.push(vblock(mincho, &mut img, "太宰治", 36.0, 50.0, r - 120.0, 1700.0, 3));
    let (size, pitch, per, right) = (34.0, 56.0, 54, r - 220.0);
    let text = fit(MEROSU, cols_between(right, size, pitch, margin), per);
    b.push(vblock(mincho, &mut img, &text, size, pitch, right, 200.0, per));
    out.push(("t1_merosu_mincho".into(), img, b));

    // t2 (Mincho): horizontal running head, four tiers of 12-character columns, page number.
    let mut img = blank();
    let mut b = vec![hline(mincho, &mut img, "高瀬舟", 26.0, margin, 80.0)];
    b.extend(tiers(mincho, &mut img, TAKASEBUNE, 30.0, 48.0, r, 200.0, 12, 12, 90.0));
    b.push(hline(mincho, &mut img, "48", 26.0, PAGE_W as f32 / 2.0 - 13.0, PAGE_H as f32 - 130.0));
    out.push(("t2_takasebune_mincho".into(), img, b));

    // t3 (Mincho): an 8-character 52 px heading, the author, large 46 px body.
    let mut img = blank();
    let mut b = vec![vblock(mincho, &mut img, "コーヒー哲学序説", 52.0, 80.0, r, 200.0, 8)];
    b.push(vblock(mincho, &mut img, "寺田寅彦", 32.0, 50.0, r - 80.0, 1800.0, 4));
    let (size, pitch, per, right) = (46.0, 74.0, 36, r - 150.0);
    let text = fit(COFFEE, cols_between(right, size, pitch, margin), per);
    b.push(vblock(mincho, &mut img, &text, size, pitch, right, 200.0, per));
    out.push(("t3_coffee_mincho".into(), img, b));

    // t4 (Gothic): a 120 px two-character title with the author under it, two tiers.
    let mut img = blank();
    let mut b = vec![vblock(gothic, &mut img, "檸檬", 120.0, 140.0, r, 220.0, 2)];
    b.push(vblock(gothic, &mut img, "梶井基次郎", 30.0, 40.0, r - 45.0, 540.0, 5));
    let (size, pitch, per, right) = (36.0, 58.0, 26, r - 200.0);
    let cols = cols_between(right, size, pitch, margin);
    b.extend(tiers(gothic, &mut img, LEMON, size, pitch, right, 200.0, per, cols, 110.0));
    out.push(("t4_lemon_gothic".into(), img, b));

    // t5 (Gothic): a dense page of 28 px, 66-character columns under a horizontal running head.
    let mut img = blank();
    let head = "セロ弾きのゴーシュ";
    let mut b = vec![hline(gothic, &mut img, head, 24.0, r - 24.0 * head.chars().count() as f32, 90.0)];
    b.push(vblock(gothic, &mut img, GAUCHE, 28.0, 44.0, r, 200.0, 66));
    out.push(("t5_gauche_gothic".into(), img, b));

    // t6 (Mincho): a 56 px heading, then one block of 40 px dialogue.
    let mut img = blank();
    let mut b = vec![vblock(mincho, &mut img, "斜陽", 56.0, 90.0, r, 200.0, 2)];
    let (size, pitch, per, right) = (40.0, 64.0, 44, r - 110.0);
    let text = fit(SHAYOU, cols_between(right, size, pitch, margin), per);
    b.push(vblock(mincho, &mut img, &text, size, pitch, right, 200.0, per));
    out.push(("t6_shayou_mincho".into(), img, b));

    // t7 (Mincho): a 64 px title and the author in one column, a body block below the title's height.
    let mut img = blank();
    let mut b = vec![vblock(mincho, &mut img, "たけくらべ", 64.0, 90.0, r, 200.0, 5)];
    b.push(vblock(mincho, &mut img, "樋口一葉", 32.0, 50.0, r - 16.0, 600.0, 4));
    let (size, pitch, per, right) = (38.0, 62.0, 40, r - 120.0);
    let text = fit(TAKEKURABE, cols_between(right, size, pitch, margin), per);
    b.push(vblock(mincho, &mut img, &text, size, pitch, right, 760.0, per));
    out.push(("t7_takekurabe_mincho".into(), img, b));
    out
}
