import { useMemo } from "react";
import { call, type Settings } from "../api";
import { deg, describe, next24, parse, type WeatherPayload } from "../lib/weather";
import { WeatherIcon } from "./WeatherIcon";

export function Weather({ weather, settings, openSettings }: { weather: WeatherPayload | null; settings: Settings; openSettings: () => void }) {
  const p = useMemo(() => (weather ? parse(weather) : null), [weather]);

  if (!settings.weatherEnabled) {
    return (
      <div className="chat-empty wx-empty">
        <WeatherIcon code={2} size={34} />
        <div className="chat-empty-title">Погода выключена</div>
        <button className="text-btn" onClick={openSettings}>
          Включить в настройках
        </button>
      </div>
    );
  }
  if (!weather || !p) {
    return (
      <div className="chat-empty wx-empty">
        <div className="ly-dots">
          <i />
          <i />
          <i />
        </div>
        <div className="hint">Загружаю прогноз…</div>
        <button className="text-btn" onClick={() => call("weather_refresh")}>
          Обновить
        </button>
      </div>
    );
  }

  const { now, hours, days } = p;
  const strip = next24(hours);
  const lo = Math.min(...days.map((d) => d.min));
  const hi = Math.max(...days.map((d) => d.max));
  const span = Math.max(1, hi - lo);
  const updated = new Date(weather.fetchedMs).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });

  return (
    <div className="wx">
      <div className="card wx-now">
        <button className="wx-city" onClick={openSettings} title="Сменить город">
          {weather.city || "Здесь"}
        </button>
        <div className="wx-main">
          <WeatherIcon code={now.code} day={now.day} size={58} />
          <div className="wx-temp">{deg(now.temp)}</div>
        </div>
        <div className="wx-desc">{describe(now.code)}</div>
        <div className="wx-facts">
          <span>
            Ощущается <b>{deg(now.feels)}</b>
          </span>
          <span>
            Ветер <b>{now.wind.toFixed(now.wind < 10 ? 1 : 0).replace(".", ",")} м/с</b>
          </span>
          <span>
            Влажность <b>{Math.round(now.humidity)}%</b>
          </span>
        </div>
        <div className="wx-updated">Обновлено в {updated} · Open-Meteo</div>
      </div>

      <div className="card wx-days">
        {days.map((d) => {
          const left = ((d.min - lo) / span) * 100;
          const width = Math.max(4, ((d.max - d.min) / span) * 100);
          return (
            <div className="wx-day" key={d.t}>
              <span className="wx-day-name">{d.label}</span>
              <WeatherIcon code={d.code} size={20} />
              <span className="wx-day-min">{deg(d.min)}</span>
              <span className="wx-bar">
                <i style={{ left: `${left}%`, width: `${width}%` }} />
              </span>
              <span className="wx-day-max">{deg(d.max)}</span>
            </div>
          );
        })}
      </div>

      <div className="card wx-hours">
        {strip.map((h, i) => (
          <div className="wx-hour" key={h.t}>
            <span className="wx-hour-t">{i === 0 ? "Сейчас" : h.label}</span>
            <WeatherIcon code={h.code} day={h.day} size={22} />
            <span className="wx-hour-temp">{deg(h.temp)}</span>
            <span className={`wx-hour-p ${h.prob >= 50 ? "wet" : ""}`}>{h.prob >= 10 ? `${h.prob}%` : ""}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

/** Collapsed island without music: icon on the left, temperature on the right. */
export function WeatherCompact({ weather }: { weather: WeatherPayload }) {
  const { now } = parse(weather);
  return (
    <div className="wx-compact">
      <WeatherIcon code={now.code} day={now.day} size={20} />
      <span>{deg(now.temp)}</span>
    </div>
  );
}
