// Hata metinleri: her hata türü için başlık ve açıklama, ayrıntı satırları,
// yazma engelleri, planın geçersizleşme nedenleri ve diğer ortak listeler.

import type { errors as en } from "../en/errors";
import type { Messages } from "../types";

/** Başka bir programın klavyede bıraktıkları, sıfır olanlar olmadan ("a, b ve c"). */
function evidenceText(p: { status: number; acks: number; profiles: number }): string {
  const parts = [
    p.status > 0 ? `${p.status} durum mesajı` : null,
    p.acks > 0 ? `başka bir programın komutlarına verilmiş ${p.acks} yanıt` : null,
    p.profiles > 0 ? `${p.profiles} profil bildirimi` : null,
  ].filter((part): part is string => part !== null);
  if (parts.length === 0) return "mesajlar";
  const last = parts.pop() ?? "";
  return parts.length === 0 ? last : `${parts.join(", ")} ve ${last}`;
}

export const errors: Messages<typeof en> = {
  kind: {
    not_found: {
      title: "Klavye bulunamadı",
      body: "OpenWraith klavyeyi bulamıyor. USB kablosunu denetleyin.",
    },
    busy: {
      title: "Klavye açılamadı",
      body: "Klavyeyi başka bir program tutuyor olabilir. Diğer klavye araçlarını kapatıp yeniden deneyin.",
    },
    other_program_active: {
      title: "Klavyeyi başka bir program kullanıyor",
      body: "Bu büyük olasılıkla wraith.software sitesi. O tarayıcı sekmesini tamamen kapatıp yeniden denetleyin.",
    },
    timeout: {
      title: "Klavye zamanında yanıt vermedi",
      body: "Kabloyu denetleyip yeniden deneyin.",
    },
    keyboard_reset: {
      title: "Klavye kendini sıfırladı",
      body: "Klavye fabrika ayarlarına döndüğünü bildirdi (Fn + Boşluk). OpenWraith durdu.",
    },
    disconnected: {
      title: "Klavyenin bağlantısı kesildi",
      body: "Yeniden takın; OpenWraith kendiliğinden bağlanır.",
    },
    profile_changed: {
      title: "Klavyede profil değişti",
      body: "İşlem sırasında klavyede profil değiştirildi (Fn + 1–4); bu yüzden OpenWraith durdu.",
    },
    changed_since_plan: {
      title: "Klavyenin ayarları değişti",
      body: "Siz değişikliği inceledikten sonra ayarlar değişti. Değişikliği yeniden inceleyin.",
    },
    verify_failed: {
      title: "Klavye değişikliği kabul etmedi",
      body: "Yazma işleminden sonra klavye, ikinci denemede bile farklı değerler döndürdü.",
    },
    not_persisted: {
      title: "Klavye değişikliği saklamadı",
      body: "Değişiklik yazıldı ama profil değiştirildikten sonra kayboldu.",
    },
    model_mismatch: {
      title: "Bu yedek başka bir klavyeye ait",
      body: "OpenWraith bir yedeği yalnızca aynı modele geri yükler.",
    },
    firmware_mismatch: {
      title: "Yedek farklı bir firmware ile alınmış",
      body: "Ayarlar başka bir firmware sürümünde farklı saklanıyor olabilir. Eminseniz yine de geri yükleyebilirsiniz.",
    },
    invalid_backup: {
      title: "Yedek dosyası kullanılamıyor",
      body: "Dosya bozuk ya da bir OpenWraith yedeği değil.",
    },
    cancelled: {
      title: "Durduruldu",
      body: "İşlem isteğiniz üzerine durduruldu.",
    },
    io: {
      title: "Bir dosya okunamadı ya da yazılamadı",
      body: "Klasörün var olduğunu ve yazma izniniz olduğunu denetleyin.",
    },
    unexpected_reply: {
      title: "Klavye beklenmeyen bir yanıt verdi",
      body: "OpenWraith önlem olarak durdu.",
    },
    invalid_request: {
      title: "Bu değişiklik yapılamaz",
      body: "Değerleri denetleyip yeniden deneyin.",
    },
    not_connected: {
      title: "Bağlı klavye yok",
      body: "Klavyeyi bağlayın ve okunmasını bekleyin.",
    },
    operation_running: {
      title: "Başka bir işlem sürüyor",
      body: "Bitmesini bekleyin.",
    },
    stale_plan: {
      title: "Klavyenin ayarları değişti",
      body: "Klavye yeniden okundu. Değişikliğinizi denetleyip yeniden inceleyin.",
    },
    needs_switch_consent: {
      title: "Karşılaştırma için profil değiştirmek gerekiyor",
      body: "Önce profil geçişlerini onaylayın.",
    },
    writes_blocked: {
      title: "Yazma duraklatıldı",
      body: "OpenWraith şu anda yazamıyor.",
    },
    safety_dir_unavailable: {
      title: "Güvenlik yedeği klasörü kullanılamıyor",
      body: "OpenWraith güvenlik yedeği kaydedemediği için yazmıyor.",
    },
    file_unknown: {
      title: "Dosyaya artık ulaşılamıyor",
      body: "Dosyayı yeniden açın.",
    },
    internal: {
      title: "OpenWraith içinde bir sorun oluştu",
      body: "Sorun sürerse teknik ayrıntılarla birlikte bildirin.",
    },
    travel_test_open: {
      title: "Bir tuş derinliği testi hâlâ çalışıyor",
      body: "Klavye hâlâ tuş derinliği gönderiyor olabilir. Önce testi durdurun, sonra yeniden deneyin.",
    },
  },
  detail: {
    other_program: (p) => `OpenWraith'in istemediği ${evidenceText(p)} geldi.`,
    profile_changed: (p) =>
      `OpenWraith profil ${p.expected} bekliyordu; klavye profil ${p.reported} bildirdi.`,
    profile_changed_reported: (p) => `Klavye profil ${p.reported} bildirdi.`,
    profile_changed_before_save: (p) =>
      `Klavye değişikliği kaydetmeden önce ${p.reported} numaralı profile geçti.`,
    firmware_mismatch: (p) => `Yedek: firmware ${p.backup}. Klavye: firmware ${p.keyboard}.`,
    model_mismatch: (p) => `Yedek: ${p.backup}. Klavye: ${p.keyboard}.`,
    unsupported_model: (p) => `Klavye kendini şöyle tanıtıyor: ${p.product}`,
    profile_not_in_backup: (p) => `Profil ${p.n} bu yedekte yok.`,
    active_profile_now: (p) => `Klavye şu an profil ${p.n} üzerinde.`,
    active_profile_unknown: "Etkin profil şu anda bilinmiyor.",
    key_not_editable: (p) => `${p.key}: ${p.reason}`,
    value_out_of_range: (p) =>
      `${p.field} izin verilen aralığın dışında: ${p.value} (en az ${p.min}, en çok ${p.max}).`,
    distance_unit: (p) =>
      `Klavye mesafe birimini ${p.um} µm olarak bildiriyor; OpenWraith yalnızca 0,02 mm'lik adımlarla yazabilir.`,
    distance_unit_unknown: "Klavyenin mesafe birimi bilinmiyor; OpenWraith yalnızca 0,02 mm'lik adımlarla yazar.",
    needs_switch_consent: (p) => `Karşılaştırma için şu profillere geçilir: ${p.list}.`,
    verify_failed: (p) => `${p.table}: ${p.sends} denemeden sonra da geri okuma farklı çıktı.`,
    commit_trigger_incomplete: (p) =>
      `Klavyenin kaydetmesini sağlayan adım ${p.of} paketten ${p.packet}. pakette durdu.`,
    io: (p) => `Dosya: ${p.file}`,
    unsupported_schema: (p) =>
      `Bu yedek OpenWraith'in daha yeni bir sürümüyle alınmış (biçim ${p.found}; bu sürüm en çok biçim ${p.supported} okuyabilir).`,
  },
  writeBlocked: {
    not_connected: "Bağlı ve okunmuş bir klavye yok",
    observing: "OpenWraith, klavyeyi başka bir programın kullanıp kullanmadığını hâlâ denetliyor",
    busy: "Başka bir işlem sürüyor",
    other_program: "Klavyeyi başka bir program kullanıyor",
    reset_seen: "Klavye kendini sıfırladı; önce yeniden okuyun",
    snapshot_stale: "Gösterilen değerler güncel olmayabilir; klavyeyi yeniden okuyun",
    safety_dir_unavailable: "Güvenlik yedeği klasörü kullanılamıyor",
    fault: "OpenWraith bir iç hatayla karşılaştı; uygulamayı yeniden başlatın",
  },
  depthViewRunning: "Tuş derinliği görünümü açık. Önce Aktivasyon > Tuş derinliği sekmesinden durdurun.",
  planInvalidation: {
    superseded: "Daha yeni bir plan onun yerini aldı.",
    expired: "10 dakikadan daha eski.",
    unknown: "OpenWraith bu planı artık tanımıyor.",
    disconnected: "Klavyenin bağlantısı kesildi.",
    reset_seen: "Klavye kendini sıfırladı.",
    other_program: "Klavyeyi başka bir program kullandı.",
    profile_changed: "Klavyede profil değişti.",
    values_changed: "Klavyedeki değerler değişti.",
    snapshot_changed: "Klavye yeniden okundu.",
    operation: "Arada başka bir işlem yapıldı.",
    resubscribed: "Pencere yeniden yüklendi.",
    discarded: "İptal edildi.",
  },
  notEditable: {
    knob: "Döner düğmenin tetikleme ayarı yok.",
    not_read: "Klavyeden okunmadı.",
    undecodable: "Kayıtlı değeri çözümlenemedi.",
    unknown_mode: (p) => `Bilinmeyen bir tetikleme modu kullanıyor (${p.mode}).`,
    second_travel_differs: "OpenWraith'in değiştirmediği ikinci bir aktivasyon mesafesi kullanıyor.",
    press_release_differ: "Rapid Trigger basma ve bırakma değerleri, aynı olmaları gereken bir modda farklı.",
  },
  rangeField: {
    actuation: "Aktivasyon noktası",
    sensitivity: "Hassasiyet",
    press: "Basma hassasiyeti",
    release: "Bırakma hassasiyeti",
    brightness: "Parlaklık",
    speed: "Hız",
  },
  intentField: {
    keys: "Hiç tuş seçilmedi.",
    knob: "Döner düğmenin tetikleme ayarı yok.",
    duplicate_key: "Bir tuş iki kez listelendi.",
    nothing_to_change: "Değiştirilecek bir şey yok.",
    mode: "Bu efekt seçilemez.",
    color: "Renk geçerli değil.",
    profiles: "Yedekten en az bir profil seçin.",
    source: "Yedek dosyasına artık ulaşılamıyor.",
  },
  mismatchKind: {
    missing_reply: "Yanıt yok",
    invalid_reply: "Geçersiz yanıt",
    differs: "Farklı değerler",
  },
  keymap: {
    keyRef: (p) => `${p.key} (#${p.index})`,
    keyRefFn: (p) => `Fn + ${p.key} (#${p.index})`,
    no_changes: "Değiştirilecek bir şey yok.",
    too_many_changes: (p) => `Bir seferde çok fazla değişiklik var (en çok ${p.max}). Önce bir kısmını yazın.`,
    not_a_key: (p) => `Bu klavyede #${p.index} numaralı tuş yok.`,
    slot_out_of_range: (p) => `Bu klavyede ${p.slot} yok.`,
    not_offered: "OpenWraith bu eylemi tuşlar için sunmuyor.",
    invalid_combo: "Bir kısayol için bir ya da iki farklı değiştirici tuş ve başka bir tuş gerekir.",
    macro_too_long: (p) => `Makroda ${p.steps} adım var; klavye en çok ${p.max} adım saklar.`,
    macro_unbalanced: (p) =>
      `Makronun ${p.step}. adımı uymuyor: basılan her tuş bir kez de bırakılmalıdır. "Sonda hepsini bırak" basılı kalan tuşları düzeltir.`,
    macro_last_delay: "Makronun son adımından sonra bekleme olamaz.",
    macro_times_zero: "Bir makro en az bir kez çalmalıdır.",
    dks_shape: "Dinamik tuş vuruşu için her biri dört adımlı bir ile dört arasında tuş gerekir.",
    dks_point: (p) => `İlk nokta (${p.value}) 0,1 mm ile ${p.max} arasında olmalıdır.`,
    dks_steps: (p) => `Dinamik tuş vuruşunun ${p.row}. satırı klavyenin kabul ettiği bir birleşim değil.`,
    mt_hold_time: (p) => `Basılı tutma süresi (${p.value}) ${p.min} ile ${p.max} arasında, 10 ms'lik adımlarla olmalıdır.`,
    swap_not_writable: (p) =>
      `${p.key} OpenWraith'in okunduğu gibi koruduğu bir değer taşıyor: yerine başka bir şey atanabilir ama yer değiştirilemez.`,
    knob: "Döner düğmeyi klavye yönetir; OpenWraith onu asla değiştirmez.",
    fn_key: "Fn tuşu yeniden atanamaz.",
    fn_locked: (p) => `${p.key}, Fn katmanında klavye tarafından kilitli.`,
    builtin_not_acknowledged: (p) =>
      `${p.key} klavyenin kendi işlevini çalıştırır (${p.function}). Yerine başka bir şey atamak istediğinizi onaylayın.`,
    builtin_not_acknowledged_critical: (p) =>
      `${p.key} klavyenin temel kısayollarından biri (${p.function}). Yerine başka bir şey atamak için açık onayınız gerekir.`,
    no_usage: (p) => `${p.key} tuşunun kendine ait bir tuş kodu yok, bu yüzden makro başlatamaz.`,
    unknown_model: "OpenWraith bu klavyenin tuş düzenini bilmiyor.",
    no_free_slot: {
      macro: "Tüm makro yuvaları dolu. Önce bir makroyu silin.",
      advanced: "Tüm gelişmiş tuş yuvaları dolu. Önce bir gelişmiş tuşu silin.",
    },
    slot_already_bound: (p) => `${p.slot} zaten ${p.key} tarafından kullanılıyor.`,
    binding_to_empty_slot: (p) => `${p.key} boş olan ${p.slot} yuvasına bağlanacaktı.`,
    new_slot_replaces_edit: (p) =>
      `${p.key} şu anda ${p.slot} yuvasını çalıştırıyor ve değişiklikleriniz bu yuvayı da düzenliyor. ${p.key} için yeni bir tane aynı yuvaya yazılır ve iki değişiklikten biri kaybolur. Önce düzenlemeyi yazın ya da onun yerine yuvayı silin.`,
    slot_still_referenced: (p) =>
      `${p.slot} hâlâ şu tuşlar tarafından kullanılıyor: ${p.keys}. Önce bu tuşları değiştirin ya da onu tuşlarıyla birlikte silin.`,
    unsupported_current_table: (p) =>
      `${p.table} OpenWraith'in birebir yeniden oluşturamayacağı bir şey içeriyor; Geri al onu geri getiremez.`,
    other: "Tuş atamaları bu şekilde değiştirilemez.",
  },
};
