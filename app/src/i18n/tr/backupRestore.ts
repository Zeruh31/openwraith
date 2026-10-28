// Yedek ve profiller sayfası (yedekleme sayfası ajanına ait): yedekleme,
// geri yükleme sihirbazı, planın incelenmesi, geri yükleme raporu, profil
// paylaşma (dışa ve içe aktarma) ve güvenlik yedeklerinin listesi.

import type { backupRestore as en } from "../en/backupRestore";
import type { Messages } from "../types";

export const backupRestore: Messages<typeof en> = {
  title: "Yedek ve profiller",
  description: "Klavyenin ayarlarını bir dosyaya kaydedin, daha sonra geri getirin ve bir profili paylaşın.",
  reasons: {
    notConnected: "Klavyeyi bağlayın ve okunmasını bekleyin.",
    busy: "Süren işlemin bitmesini bekleyin.",
    otherProgram:
      "Klavyeyi başka bir program kullanıyor. Bütün wraith.software sekmelerini kapatın, sonra yeniden denetleyin.",
    resetSeen: "Klavye kendini sıfırladı. Önce klavyeyi yeniden okuyun.",
    noProfile: "En az bir profil seçin.",
    otherKeyboard: "Bu yedek başka bir klavyeye ait.",
    firmwareTick: "Bu yedeği karşılaştırmak için “Yine de geri yükle” kutusunu işaretleyin.",
  },
  dialog: {
    filter: "OpenWraith yedekleri",
    saveAll: "Tüm profillerin yedeğini kaydet",
    saveActive: "Etkin profilin yedeğini kaydet",
    open: "Geri yüklenecek yedeği seçin",
  },
  backup: {
    title: "Yedekle",
    intro: "Yedek dosyası klavyenin ayarlarını içerir: aktivasyon, Rapid Trigger, ışık, tuş atamaları ve daha fazlası.",
    all: "Dört profilin hepsini yedekle…",
    recommended: "Önerilen",
    allHint:
      "Okuma sırasında klavye profilleri tek tek dolaşır (ışık kısa süre değişir), sonra okuma başladığında bulunduğu profile geri döner. Hiçbir ayar değiştirilmez.",
    active: "Yalnızca etkin profili yedekle…",
    activeHint: (p) => `Profil değiştirmeden profil ${p.n} okunur.`,
    activeHintUnknown: "Profil değiştirmeden etkin profil okunur.",
    reviewTitle: "Dört profilin hepsi yedeklensin mi?",
    reviewSwitches: (p) =>
      `Klavye sırayla ${p.visits} numaralı profillere geçer (ışık kısa süre değişir), ardından ${p.original} numaralı profile döner. Hiçbir ayar değiştirilmez.`,
    reviewNoSwitches: "Yalnızca etkin profilin okunması gerekiyor; profil değiştirilmez.",
    reviewSaveWait: (p) => `OpenWraith önce klavyenin daha önceki bir değişikliği kaydetmesi için ${p.seconds} bekler.`,
    reviewStop: "İstediğiniz an durdurabilirsiniz; bu durumda da klavye, yedekleme başladığında bulunduğu profile geri döner.",
    reviewFile: "Dosya",
    confirm: "Tüm profilleri yedekle",
    saved: "Yedek kaydedildi",
    noFileSaved: "Yedek dosyası kaydedilmedi.",
    savedSummary: (p) => `${p.profiles} · firmware ${p.firmware}`,
    savedAt: (p) => `Oluşturulma: ${p.date}.`,
    file: "Dosya:",
    undecoded: (p) => `Klavyenin yanıtlarından ${p.n} tanesi çözümlenemedi; okunduğu gibi kaydedildi.`,
    foreignActivity:
      "Klavye okunurken başka bir program klavyeyle konuştu. Bütün wraith.software sekmelerini kapatın ve emin olmak için yeniden yedekleyin.",
  },
  profiles: {
    all: (p) => `Tüm profiller (${p.range})`,
    one: (p) => `Yalnızca profil ${p.n}`,
    some: (p) => `Profiller: ${p.list}`,
  },
  restore: {
    title: "Geri yükle",
    intro:
      "Bir yedeğin ayarlarını klavyeye geri getirin. OpenWraith önce karşılaştırır ve neyin değişeceğini gösterir; siz onaylamadan hiçbir şey yazılmaz.",
    step1: "Yedeği seçin",
    step2: "Profilleri seçin",
    step3: "Klavyeyle karşılaştırın",
    openFile: "Dosya aç…",
    openOther: "Başka bir dosya aç…",
    orSafety: "Aşağıda listelenen güvenlik yedeklerinden birini de geri yükleyebilirsiniz.",
    opening: "Yedek açılıyor…",
    kinds: {
      user: "Yedek dosyası",
      safety: "Güvenlik yedeği",
    },
    facts: {
      made: "Oluşturulma",
      keyboard: "Klavye",
      firmware: "Firmware",
      profiles: "Profiller",
      tool: "Oluşturan program",
      reason: "Neden",
    },
    simulator: "Geliştirici simülatörüyle oluşturulmuş: gerçek bir klavyenin ayarlarını içermez.",
    undecoded: (p) => `Bu yedek oluşturulurken yanıtlardan ${p.n} tanesi çözümlenemedi.`,
    otherKeyboardTitle: "Bu yedek başka bir klavyeye ait",
    otherKeyboardBody: (p) =>
      `Şu modelde oluşturulmuş: ${p.model}. OpenWraith bir yedeği yalnızca aynı modele geri yükler.`,
    notChecked: "Bağlı bir klavye yok; bu yüzden OpenWraith bu yedeğin klavyeye uyup uymadığını henüz denetlemedi.",
    notInBackup: "Bu yedekte yok",
    firmwareTitle: "Bu yedek farklı bir firmware sürümüyle alınmış",
    firmwareBody: (p) =>
      `Yedek: firmware ${p.backup}. Klavye: firmware ${p.keyboard}. Firmware güncellemesinden sonra geri yüklemek olağan bir durumdur, ama bazı ayarların biçimi sürümler arasında değişmiş olabilir.`,
    firmwareAnyway: "Yine de geri yükle",
    firmwareAnywayHint: "OpenWraith yine önce karşılaştırır ve bir şey yazmadan önce her değişikliği gösterir.",
    compareHint: "OpenWraith klavyeyi okur ve neyin değişeceğini gösterir. Siz onaylamadan hiçbir şey yazılmaz.",
    compare: "Klavyeyle karşılaştır…",
    consentTitle: "Karşılaştırmak için profil değiştirilsin mi?",
    consentBody: (p) =>
      p.count === 1
        ? `Karşılaştırma için ${p.list} numaralı profil okunur: klavye bu profile geçer ve ${p.active} numaralı profile geri döner (ışık kısa süre değişir). Hiçbir ayar yazılmaz.`
        : `Karşılaştırma için ${p.list} numaralı profiller okunur: klavye bu profillere sırayla geçer ve ${p.active} numaralı profile geri döner (ışık kısa süre değişir). Hiçbir ayar yazılmaz.`,
    consentBodyUnknown: (p) =>
      p.count === 1
        ? `Karşılaştırma için ${p.list} numaralı profil okunur: klavye bu profile geçer ve geri döner (ışık kısa süre değişir). Hiçbir ayar yazılmaz.`
        : `Karşılaştırma için ${p.list} numaralı profiller okunur: klavye bu profillere sırayla geçer ve geri döner (ışık kısa süre değişir). Hiçbir ayar yazılmaz.`,
    consentConfirm: "Profil değiştir ve karşılaştır",
  },
  review: {
    title: "Bu yedek geri yüklensin mi?",
    undoTitle: (p) =>
      p.count === 1
        ? `Geri al: profil ${p.list}, ${p.date} tarihindeki durumuna döndürülsün mü?`
        : `Geri al: ${p.list} numaralı profiller, ${p.date} tarihindeki durumlarına döndürülsün mü?`,
    undoNote: (p) =>
      p.count === 1
        ? `Bu işlem profilin tamamını o zamanki durumuna döndürür; profil ${p.list} üzerinde sonradan yapılan değişiklikler de geri alınır.`
        : `Bu işlem bu profilleri o zamanki durumlarına döndürür; ${p.list} numaralı profillerde sonradan yapılan değişiklikler de geri alınır.`,
    facts: {
      file: "Yedek",
      backupMade: "Yedeğin tarihi",
      keyboardRead: "Klavyenin okunduğu an",
      activeProfile: "Etkin profil",
    },
    madeWith: (p) => `${p.date} · firmware ${p.firmware}`,
    returnsTo: (p) => `Profil ${p.n}; klavye sonunda bu profile döner`,
    switched: (p) =>
      p.count === 1
        ? `Karşılaştırmak için klavye ${p.list} numaralı profile geçip geri döndü.`
        : `Karşılaştırmak için klavye ${p.list} numaralı profillere geçip geri döndü.`,
    profileSummary: (p) => (p.changes === 0 ? `Profil ${p.n}: değişiklik yok` : `Profil ${p.n}: ${p.changes} değişiklik`),
    profileSkipped: (p) => `${p.n} ayar geri yüklenmeyecek`,
    notCompared: "Bu profil klavyeyle karşılaştırılmadı.",
    willWrite: "Yazılacak",
    notRestored: "Geri yüklenmeyecek",
    alreadyMatches: (p) => `Zaten aynı (${p.n})`,
    notInBackup: (p) => `Yedekte yok (${p.n})`,
    triggers: (p) => `Aktivasyon ve Rapid Trigger: ${p.n} tuş`,
    columns: {
      key: "Tuş",
      now: "Şu an",
      backup: "Yedekte",
      setting: "Ayar",
    },
    macros: (p) => `Makrolar ${p.list}`,
    advancedKeys: (p) => `Gelişmiş tuşlar ${p.list}`,
    commitNote: "Bunun yazılması, klavyenin tuş atamalarını kaydetmesini de sağlar.",
    moreChanges: (p) => `${p.n} değişiklik daha gösterilmiyor.`,
    totals: (p) =>
      p.count === 1
        ? `Profil ${p.list} üzerinde ${p.n} değişiklik`
        : `${p.list} numaralı profillerde toplam ${p.n} değişiklik`,
    packets: (p) => `Toplam ${p.writes} tablo yazımı, ${p.packets} paket`,
    packetsProfile: (p) => `Profil ${p.n}: ${p.writes} tablo yazımı, ${p.packets} paket`,
    confirm: (p) => `${p.n} değişikliği geri yükle`,
    nothing: "Yazılacak bir şey yok: klavye bu yedekle zaten aynı.",
    nothingSkips: "Yedeğin şu bölümleri geri yüklenemez:",
    safetyNote: (p) =>
      p.count === 1
        ? `Önce profil ${p.list} için bir güvenlik yedeği şu klasöre kaydedilecek: ${p.dir}`
        : `Önce ${p.list} numaralı profiller için bir güvenlik yedeği şu klasöre kaydedilecek: ${p.dir}`,
  },
  changes: {
    rtOff: "RT kapalı",
    rt: (p) => `RT ${p.value}`,
    rtSeparate: (p) => `RT ${p.press} / ${p.release}`,
    unknownMode: (p) => `Bilinmeyen mod ${p.mode}`,
    trigger: (p) => `${p.actuation} · ${p.rt}`,
    raw: "Çözümlenmemiş değer",
    light: {
      effect: "Efekt",
      color: "Renk",
      brightness: "Parlaklık",
      speed: "Hız",
    },
    unknownEffect: (p) => `Bilinmeyen efekt (${p.id})`,
    keymapKeys: (p) => `${p.n} tuş farklı (${p.keys})`,
    deadBand: (p) => `üst ${p.top}, alt ${p.bottom}`,
    deadBandNone: "ayarlanmamış",
    colors: (p) => `${p.n} tuşun rengi farklı`,
    colorNone: "yok",
    macroEmpty: "boş",
    macroSteps: (p) => `${p.n} adım`,
    macroInUse: "kullanımda",
    advancedEmpty: "boş",
    advancedInUse: (p) => `kullanımda (tür ${p.kind})`,
    entries: (p) => `${p.n} kayıt farklı`,
  },
  result: {
    headline: {
      done: (p) =>
        p.n === 1
          ? "Geri yükleme tamamlandı. Değişiklik yazıldı, doğrulandı ve klavye onu sakladı (profil değişiminden sonra denetlendi)."
          : `Geri yükleme tamamlandı. ${p.n} değişikliğin hepsi yazıldı, doğrulandı ve klavye bunları sakladı (profil değişiminden sonra denetlendi).`,
      doneUnchecked: (p) =>
        p.n === 1
          ? "Geri yükleme tamamlandı. Değişiklik yazıldı ve doğrulandı."
          : `Geri yükleme tamamlandı. ${p.n} değişiklik yazıldı ve doğrulandı.`,
      nothingToDo: "Yazılacak bir şey yoktu: klavye bu yedekle zaten aynıydı.",
      cancelled: (p) => `Durduruldu. Durdurmadan önce ${p.written} değişiklik yazıldı; ${p.rest} değişiklik yazılmadı.`,
      notWritten: "Hiçbir şey yazılmadı.",
      failed: (p) =>
        `Geri yükleme bir hatayla durdu. ${p.total} değişiklikten ${p.written} tanesi yazıldı ve doğrulandı; geri kalanı yazılmadı.`,
      maybeLost: (p) =>
        `Yazıldı ve doğrulandı, ama klavye ${p.n} değişikliği kaydetmemiş olabilir. Denetlemek için klavyeyi yeniden okuyun ya da geri alın.`,
      lost: (p) => `Klavye ${p.n} değişikliği saklamadı. Geri yüklemeyi yeniden çalıştırın ya da geri alın.`,
    },
    persistenceTitle: "Kalıcılık denetimi",
    persistence: {
      profiles: (p) =>
        p.count === 1
          ? `Başka bir profile geçildikten sonra profil ${p.list} yeniden seçildi.`
          : `Başka bir profile geçildikten sonra ${p.list} numaralı profiller yeniden seçildi.`,
      kept: (p) => `${p.n} değişiklik, profile geri dönüldüğünde hâlâ yerindeydi.`,
      lost: (p) => `${p.n} değişiklik, profile geri dönüldüğünde kaybolmuştu.`,
      notChecked: (p) =>
        `${p.n} değişiklik denetlenemedi: yazıldıktan sonra SOCD klavyeden (Fn + Sağ Shift) açıldı ya da kapatıldı.`,
    },
    finalTitle: "Son denetim",
    finalOk: (p) =>
      p.count === 1
        ? `SOCD tüm profillerde birlikte açılıp kapanır. Profil ${p.list} yeniden okundu; geri yüklenen ayar yerinde.`
        : `SOCD tüm profillerde birlikte açılıp kapanır. ${p.list} numaralı profiller yeniden okundu; geri yüklenen ayar yerinde.`,
    finalMismatches:
      "SOCD tüm profillerde birlikte açılıp kapanır, ama bu profilde geri yüklenen ayar yok (başka bir program değiştirmiş olabilir):",
    finalMismatch: (p) => `Profil ${p.n}: ${p.table}`,
    clearedTitle: "Boş kalan makrolar",
    clearedBody:
      "Makro 1 yazılınca klavye Makro 2–10'u siler; geri yükleme bunları yeniden yazamadan durdu. Geri al, bunları geri getirir.",
    clearedItem: (p) => `Profil ${p.n}: Makro ${p.slot}`,
    danglingTitle: "Artık hiçbir şey yapmayan tuşlar",
    danglingBody:
      "Geri yükleme yarıda kaldığı için bu tuşlar boş bir makroya ya da gelişmiş tuşa bağlı. Geri al, bunları geri getirir.",
    danglingItem: (p) => `Profil ${p.n}: ${p.key} → ${p.target}`,
    counts: {
      title: "Sayılarla",
      written: "Yazıldı ve doğrulandı",
      kept: "Profil değişiminden sonra korundu",
      maybeLost: "Kaydedilmemiş olabilir",
      lost: "Kayboldu",
      failed: "Başarısız",
      notAttempted: "Yazılmadı",
      alreadyMatched: "Zaten aynıydı",
      resent: "Değiştirilmeden yeniden gönderildi",
      skipped: "Geri yüklenmedi",
    },
    nextSteps: "Sonraki adımlar",
    runAgain: "Geri yüklemeyi yeniden çalıştır",
    runAgainHint: "Yeniden çalıştırmak için Yedek ve profiller sayfasında yedeği seçip klavyeyle karşılaştırın.",
    connectFirst: "Önce klavyeyi bağlayın.",
  },
  safety: {
    title: "Güvenlik yedekleri",
    intro:
      "OpenWraith her değişiklikten önce otomatik olarak bir tane kaydeder. Birini geri yüklemek, o zamandan beri yapılan değişiklikleri geri alır.",
    empty: "Henüz güvenlik yedeği yok. OpenWraith her değişiklikten önce otomatik olarak bir tane oluşturur.",
    loadFailed: "Güvenlik yedeklerinin listesi okunamadı.",
    columns: {
      made: "Oluşturulma",
      reason: "Neden",
      keyboard: "Klavye",
      actions: "İşlemler",
    },
    restore: "Geri yükle…",
    restoreRow: (p) => `${p.date} tarihli güvenlik yedeğini geri yükle (${p.reason})`,
    showRow: (p) => `${p.date} tarihli güvenlik yedeğini klasöründe göster`,
    refresh: "Listeyi yenile",
    showFolder: "Klasörü göster",
    count: (p) => `${p.total} yedekten ${p.shown} tanesi gösteriliyor`,
  },
  skipReasons: {
    unconfirmed_command: "OpenWraith bu ayarı henüz yazmıyor: yazılması klavyede doğrulanmadı.",
    no_known_command: "OpenWraith bu ayarı nasıl yazacağını bilmiyor.",
    unsupported_light_mode: (p) => `Yedekte yazılamayan bir ışık efekti var (${p.mode}).`,
    invalid_light_record: "Yedekteki ışık ayarları geçersiz.",
    knob: "Döner düğmenin tetikleme ayarı yok.",
    unknown_trigger_mode: (p) => `Yedekte bilinmeyen bir tetikleme modu var (${p.mode}).`,
    second_travel_differs: "Tuş, OpenWraith'in değiştirmediği ikinci bir aktivasyon mesafesi kullanıyor.",
    press_release_differ: "Tuşun Rapid Trigger basma ve bırakma değerleri, aynı olmaları gereken bir modda farklı.",
    out_of_range: "Yedekteki bir değer klavyenin kabul ettiği aralığın dışında.",
    trigger_limits_unknown: "Klavyenin tetikleme sınırları bilinmiyor.",
    dead_band_out_of_range: "Yedekteki bir ölü bölge değeri klavyenin kabul ettiği aralığın dışında.",
    dead_band_unit_unknown: "Klavyenin ölü bölge birimi bilinmiyor.",
    distance_unit_changed: (p) => `Mesafe birimi farklı (yedek: ${p.backup}, klavye: ${p.keyboard}).`,
    keymap_group_blocked: (p) =>
      `Tuş atamaları, makrolar ve gelişmiş tuşlar ancak birlikte geri yüklenebilir ve burada bu güvenli değil: ${p.cause}`,
    not_in_keyboard: "Klavyede bu ayar yok.",
    unusable: "Yedeğin bu bölümü kullanılamıyor.",
    read_only_bytes: "Yalnızca OpenWraith'in hiç yazmadığı değerler farklı (SOCD kısayolu).",
    unwritable_socd_pairs: "Yedekteki SOCD çiftleri OpenWraith'in yazabileceği biçimde değil.",
    not_imported: "Bu ayar içe aktarmaya dahil değil.",
  },
  blockCauses: {
    unwritable_advanced_key: (p) =>
      `yedekteki ${p.slot} numaralı gelişmiş tuş, OpenWraith'in yazabildiği bir DKS, dokun ya da basılı tut veya aç/kapa tuşu değil.`,
    non_canonical_macro: (p) => `yedekteki ${p.slot} numaralı makro alışılmadık bir biçimde.`,
    malformed_reference: (p) => `yedekteki ${p.layer} içinde bozuk bir kayıt var (${p.index} numaralı).`,
    incomplete: (p) => `şu bölüm tam okunamadı: ${p.table}.`,
    fn_keymap_not_writable: "Fn katmanı tuş atamaları yazılamıyor; bu yüzden klavye tuş atamalarını kaydetmez.",
    critical_fn_override: (p) =>
      `içe aktarılan Fn katmanı klavyenin kendi kısayollarından birinin yerini alıyor (profil değiştirme ya da sıfırlama, ${p.index} numaralı tuş) ve buna izin verilmedi.`,
    unwritable_imported_entry: (p) =>
      `dosyadaki ${p.layer} içinde OpenWraith'in yazmadığı bir kayıt var (${p.index} numaralı tuş). Yalnızca tuşlar, medya tuşları, fare düğmeleri, tuş kombinasyonları, makrolar ve gelişmiş tuşlar içe aktarılır.`,
  },
  layers: {
    base: "tuş atamaları",
    fn: "Fn katmanı tuş atamaları",
  },
  planWarnings: {
    firmware_differs: (p) => `Yedek ${p.backup} sürümlü firmware ile alınmış; klavyede ${p.keyboard} sürümü var.`,
    constant_differs: "Normalde hiç değişmeyen bir değer yedekle klavye arasında farklı.",
    capability_flags_differ: "Klavye, yedeğin alındığı zamandan farklı özellikler bildiriyor.",
    socd_power_differs_in_backup: (p) =>
      `Yedekteki profillerde SOCD farklı biçimde açılıp kapatılmış, ama klavyede tüm profiller için tek bir SOCD anahtarı var. Yedek alınırken etkin olan profilin (profil ${p.profile}) ayarı geri yüklenir: SOCD ${p.on ? "açık" : "kapalı"}.`,
    socd_power_not_imported: (p) =>
      `Yalnızca SOCD tuş çiftleri içe aktarılır. SOCD'nin açık olup olmadığı bir profile değil tüm profillere ait tek bir ayardır; bu yüzden ${p.on ? "açık" : "kapalı"} kalır.`,
    imported: (p) => `Dosyadaki profil ${p.from}, klavyedeki profil ${p.into} üzerine yazılır.`,
    imported_macros: (p) => `Dosyadaki makrolar yazılır: ${p.slots}.`,
    fn_builtin_overridden: (p) =>
      p.kind === "reset"
        ? `Fn katmanı, basılı tutulunca klavyeyi sıfırlayan Fn + ${p.key} kısayolunun yerini alıyor. Yerine başka bir şey atanınca ne olacağı test edilmedi.`
        : p.kind === "profile"
          ? `Fn katmanı, klavyenin ${p.key} numaralı profile geçme kısayolu olan Fn + ${p.key} kısayolunun yerini alıyor.`
          : `Fn katmanı klavyenin kendi Fn kısayollarından birinin yerini alıyor: Fn + ${p.key}.`,
    import_kept_as_read: (p) =>
      `Dosyadaki ${p.layer}, hiçbir zaman içe aktarılmayan konumlarda farklı (döner düğme, Fn tuşu, klavyenin kilitlediği tuşlar): ${p.keys}. Klavyedeki kayıtlar korunur.`,
    unverified: (p) =>
      `${p.layer} içindeki içe aktarılan bir kayıt (${p.index} numaralı tuş) henüz bir W75'te doğrulanmadı. Wraith web sitesi de aynı baytları gönderir.`,
  },
  safetyReasons: {
    set_trigger: "Aktivasyon değişikliğinden önce",
    set_light: "Işık değişikliğinden önce",
    restore: "Geri yüklemeden önce",
    set_keys: "Tuş değişikliğinden önce",
    set_colors: "Tuş rengi değişikliğinden önce",
    set_deadband: "Ölü bölge değişikliğinden önce",
    set_socd: "SOCD değişikliğinden önce",
    import: "İçe aktarmadan önce",
    other: "Güvenlik yedeği",
  },
  share: {
    title: "Profil paylaş",
    intro:
      "Bir profili paylaşmak ya da saklamak için dosyaya kaydedin; bir profil dosyasını da klavyenin istediğiniz profiline içe aktarın.",
    noSerial: "Profil dosyası klavyenin USB seri numarasını hiçbir zaman içermez.",
    dialog: {
      save: "Profil dosyasını kaydet",
      saveFilter: "OpenWraith profil dosyaları",
      open: "İçe aktarılacak profil dosyasını seçin",
      openFilter: "OpenWraith profil dosyaları ve yedekleri",
    },
    profileOption: (p) => (p.active ? `Profil ${p.n} (etkin)` : `Profil ${p.n}`),
    parts: {
      triggers: "Aktivasyon ve Rapid Trigger",
      lighting: "Işık ve tuş renkleri",
      keys: "Tuş atamaları, makrolar ve gelişmiş tuşlar",
      dead_band: "Ölü bölge",
      socd: "SOCD tuş çiftleri",
    },
    export: {
      title: "Profili dışa aktar",
      intro: "OpenWraith profili şimdi klavyeden okur ve bir profil dosyası (.owprofile.json) olarak kaydeder.",
      profile: "Profil",
      name: "Ad (isteğe bağlı)",
      nameHint: "Dosyaya kaydedilir, örneğin “Turnuva”. Dosya adını bir sonraki adımda seçersiniz.",
      holds:
        "Dosya profilin tamamını içerir: aktivasyon ve Rapid Trigger, ışık ve tuş renkleri, makrolar ve gelişmiş tuşlarla birlikte tuş atamaları, ölü bölge ve SOCD tuş çiftleri. Hangi bölümlerin kullanılacağını içe aktaran kişi seçer; varsayılan olarak yalnızca aktivasyon ve ışık seçilidir.",
      macrosNote: "Makrolar bastıkları tuşlar olarak kaydedilir; dosyayı verdiğiniz herkes bunları okuyabilir.",
      switchNote: (p) =>
        `${p.n} numaralı profili okumak için klavye bu profile geçip geri döner (ışık kısa süre değişir). Hiçbir ayar değiştirilmez.`,
      button: (p) => `${p.n} numaralı profili dışa aktar…`,
      consentTitle: "Okumak için profil değiştirilsin mi?",
      consentBody: (p) =>
        `${p.n} numaralı profili okumak için klavye bu profile geçer ve ${p.active} numaralı profile geri döner (ışık kısa süre değişir). Hiçbir ayar değiştirilmez.`,
      consentBodyUnknown: (p) =>
        `${p.n} numaralı profili okumak için klavye bu profile geçip geri döner (ışık kısa süre değişir). Hiçbir ayar değiştirilmez.`,
      consentConfirm: "Profil değiştir ve dışa aktar",
      saved: "Profil dosyası kaydedildi",
      savedSummary: (p) => `Profil ${p.n} · firmware ${p.firmware}`,
      switched: (p) => `Okumak için klavye ${p.n} numaralı profile geçip geri döndü.`,
      file: "Dosya:",
      noFileSaved: "Profil dosyası kaydedilmedi.",
    },
    contents: {
      title: "Dosyanın içeriği",
      titleOf: (p) => `Dosyadaki ${p.n} numaralı profilin içeriği`,
      otherProfile: (p) => `İnceleme, dosyadaki ${p.n} numaralı profilin içeriğini gösterir.`,
      none: "OpenWraith'in okuyabildiği bir ayar yok.",
      missing: (p) => `Dosyada olmayanlar: ${p.list}.`,
      advanced: (p) => `${p.n} gelişmiş tuş kullanımda.`,
      macrosTitle: "Makrolar, bastıkları tuşlarla",
      macro: (p) => `Makro ${p.n} · ${p.steps} adım · ${p.play}`,
      macroUnknown: (p) =>
        `Makro ${p.n} adım adım gösterilemiyor; bu yüzden bu dosyadan tuş atamaları, makrolar ve gelişmiş tuşlar içe aktarılamaz.`,
      press: (p) => `${p.key} tuşuna basılır`,
      release: (p) => `${p.key} tuşu bırakılır`,
      wait: (p) => `${p.step}, ardından ${p.ms} ms beklenir`,
    },
    import: {
      title: "Profili içe aktar",
      intro:
        "Bir profil dosyasının ayarlarını klavyenin profillerinden birine yazın. OpenWraith önce karşılaştırır ve neyin değişeceğini gösterir; siz onaylamadan hiçbir şey yazılmaz.",
      open: "Profil dosyası aç…",
      openOther: "Başka bir profil dosyası aç…",
      backupsToo: "Bir yedek dosyası da seçebilirsiniz: önce yedeğin etkin profili önerilir.",
      facts: {
        made: "Oluşturulma",
        keyboard: "Klavye",
        firmware: "Firmware",
        profiles: "Dosyadaki profiller",
        tool: "Oluşturan program",
      },
      simulator: "Geliştirici simülatörüyle oluşturulmuş: gerçek bir klavyenin ayarlarını içermez.",
      otherKeyboardTitle: "Bu dosya başka bir klavyeye ait",
      otherKeyboardBody: (p) =>
        `Şu modelde oluşturulmuş: ${p.model}. OpenWraith bir profili yalnızca aynı modele içe aktarır.`,
      notChecked: "Bağlı bir klavye yok; bu yüzden OpenWraith bu dosyanın klavyeye uyup uymadığını henüz denetlemedi.",
      from: "Dosyadaki profil",
      into: "İçe aktarılacak profil",
      intoHint: (p) =>
        `Karşılaştırma için ${p.n} numaralı profil okunur: klavye bu profile geçip geri döner (ışık kısa süre değişir).`,
      parts: "Neler içe aktarılsın",
      partsHint:
        "Varsayılan olarak aktivasyon ve ışık seçilidir. Diğer bölümleri yalnızca dosyadakiyle tıpatıp aynı olmalarını istiyorsanız seçin.",
      notInFile: "Bu dosyada yok.",
      keysHint: "Profilin tuş atamalarının, makrolarının ve gelişmiş tuşlarının tamamının yerini alır.",
      socdHint: "SOCD'nin açık olup olmadığı tüm klavyeye ait tek bir ayardır: olduğu gibi kalır.",
      firmwareTitle: "Bu dosya farklı bir firmware sürümüyle oluşturulmuş",
      firmwareBody: (p) =>
        `Dosya: firmware ${p.file}. Klavye: firmware ${p.keyboard}. Bazı ayarların biçimi sürümler arasında değişmiş olabilir.`,
      firmwareAnyway: "Yine de içe aktar",
      firmwareAnywayHint: "OpenWraith yine önce karşılaştırır ve bir şey yazmadan önce her değişikliği gösterir.",
      review: "Karşılaştır ve incele…",
      compareHint: "OpenWraith klavyeyi okur ve neyin değişeceğini gösterir. Siz onaylamadan hiçbir şey yazılmaz.",
    },
    reasons: {
      noPart: "İçe aktarılacak en az bir bölüm seçin.",
      otherKeyboard: "Bu dosya başka bir klavyeye ait.",
      firmwareTick: "Bu dosyayı karşılaştırmak için “Yine de içe aktar” kutusunu işaretleyin.",
    },
    review: {
      title: (p) => `Dosyadaki ${p.from} numaralı profil, klavyedeki ${p.into} numaralı profile içe aktarılsın mı?`,
      chosen: (p) => `Seçilen bölümler: ${p.list}.`,
      confirm: (p) => `${p.n} değişikliği içe aktar`,
      safetyNote: (p) => `Önce profil ${p.n} için bir güvenlik yedeği şu klasöre kaydedilecek: ${p.dir}`,
      file: "Dosya",
      fileMade: "Dosyanın tarihi",
      columnFile: "Dosyada",
      profileSkipped: (p) => `${p.n} ayar içe aktarılmayacak`,
      notImported: "İçe aktarılmayacak",
      notInFile: (p) => `Dosyada yok (${p.n})`,
      nothing: "Yazılacak bir şey yok: klavye, dosyanın seçilen bölümleriyle zaten aynı.",
      nothingSkips: "Dosyanın şu bölümleri içe aktarılamaz:",
      keymapGroupBlocked: (p) =>
        `Tuş atamaları, makrolar ve gelişmiş tuşlar ancak birlikte içe aktarılabilir ve burada bu güvenli değil: ${p.cause}`,
      blocked:
        "Seçilen bölümlerin bazıları olduğu gibi içe aktarılamıyor; nedenini aşağıdaki “İçe aktarılmayacak” bölümü açıklıyor.",
    },
    critical: {
      title: "Dosya klavyenin kendi kısayollarından birinin yerini alıyor",
      profile: (p) =>
        p.k === p.n
          ? `Dosyada Fn + ${p.k} başka bir iş yapıyor. Klavyede Fn + ${p.k}, içe aktardığınız profil olan ${p.k} numaralı profile geçer; bu yüzden bu profilde hiçbir yere geçmez ve yerine başka bir şey atamak bir şey kaybettirmez. Yine de onayınız gerekir.`
          : `Dosyada Fn + ${p.k} başka bir iş yapıyor. Klavyede Fn + ${p.k}, ${p.k} numaralı profile geçer; bu içe aktarmadan sonra ${p.n} numaralı profil etkinken ${p.k} numaralı profile klavyeden geçemezsiniz, yalnızca OpenWraith'ten geçebilirsiniz.`,
      reset:
        "Dosyada Fn + Boşluk başka bir iş yapıyor. Klavyede Fn + Boşluk (basılı tutulunca) klavyeyi sıfırlar. Yerine başka bir şey atanınca ne olacağı test edilmedi.",
      other: (p) => `Dosyada Fn + ${p.key}, klavyenin kendi kısayollarından birinin yerini alıyor.`,
      notImported: "Tuş atamaları, makrolar ve gelişmiş tuşlar yalnızca aşağıda izin verirseniz içe aktarılır.",
      confirmProfile: (p) =>
        p.k === p.n
          ? `Yine de içe aktar: ${p.n} numaralı profilde Fn + ${p.k} değiştirilsin (yalnızca bu profile geçer)`
          : `Yine de içe aktar: ${p.n} numaralı profil etkinken Fn + ${p.k} artık ${p.k} numaralı profile geçmeyecek`,
      confirmReset: (p) => `Yine de içe aktar: ${p.n} numaralı profil etkinken Fn + Boşluk artık klavyeyi sıfırlamayacak`,
      confirmOther: (p) => `Yine de içe aktar: ${p.n} numaralı profil etkinken Fn + ${p.key} artık kendi işini yapmayacak`,
      confirmAll: (p) =>
        `Yine de içe aktar: ${p.n} numaralı profilde ${p.keys} değiştirilsin; sonuçları yukarıda açıklandı`,
      fnKey: (p) => `Fn + ${p.key}`,
    },
    result: {
      headline: {
        done: (p) =>
          p.n === 1
            ? "İçe aktarma tamamlandı. Değişiklik yazıldı, doğrulandı ve klavye onu sakladı (profil değişiminden sonra denetlendi)."
            : `İçe aktarma tamamlandı. ${p.n} değişikliğin hepsi yazıldı, doğrulandı ve klavye bunları sakladı (profil değişiminden sonra denetlendi).`,
        doneUnchecked: (p) =>
          p.n === 1
            ? "İçe aktarma tamamlandı. Değişiklik yazıldı ve doğrulandı."
            : `İçe aktarma tamamlandı. ${p.n} değişiklik yazıldı ve doğrulandı.`,
        nothingToDo: "Yazılacak bir şey yoktu: klavye, dosyanın seçilen bölümleriyle zaten aynıydı.",
        cancelled: (p) => `Durduruldu. Durdurmadan önce ${p.written} değişiklik yazıldı; ${p.rest} değişiklik yazılmadı.`,
        notWritten: "Hiçbir şey yazılmadı.",
        failed: (p) =>
          `İçe aktarma bir hatayla durdu. ${p.total} değişiklikten ${p.written} tanesi yazıldı ve doğrulandı; geri kalanı yazılmadı.`,
        maybeLost: (p) =>
          `Yazıldı ve doğrulandı, ama klavye ${p.n} değişikliği kaydetmemiş olabilir. Denetlemek için klavyeyi yeniden okuyun ya da geri alın.`,
        lost: (p) => `Klavye ${p.n} değişikliği saklamadı. İçe aktarmayı yeniden çalıştırın ya da geri alın.`,
      },
      clearedBody:
        "Makro 1 yazılınca klavye Makro 2–10'u siler; içe aktarma bunları yeniden yazamadan durdu. Geri al, bunları geri getirir.",
      danglingBody:
        "İçe aktarma yarıda kaldığı için bu tuşlar boş bir makroya ya da gelişmiş tuşa bağlı. Geri al, bunları geri getirir.",
      skipped: "İçe aktarılmadı",
      runAgain: "İçe aktarmayı yeniden çalıştır",
    },
    refusals: {
      no_changes: "İçe aktarılacak en az bir bölüm seçin.",
      not_available: "Bu, bu klavyede kullanılamıyor.",
      unsupported_current_table: (p) => `OpenWraith, dosyadaki şu ayarı olduğu gibi içe aktaramıyor: ${p.table}.`,
      other: "OpenWraith bunu, klavyeye hiçbir şey gönderilmeden önce reddetti.",
    },
  },
};
