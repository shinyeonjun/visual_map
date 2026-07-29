# Visual Map 공식 지원 스택 계약

상태: 확정 범위 1.0

이 문서는 Visual Map이 정적 분석과 기능 중심 시각화의 대상으로 공식 지원하는
언어·프레임워크·ORM·공통 기술·데이터베이스 범위를 고정한다.

이 목록 밖의 파일을 읽을 수 있는 것과 제품이 해당 언어의 의미 분석을 지원하는
것은 다르다. Tree-sitter grammar가 존재하더라도 아래 목록 밖의 언어는 Visual Map
정밀 분석 지원으로 간주하지 않는다.

## 1. 지원 언어

지원 언어는 다음 14개로 고정한다.

| 언어 | 비고 |
| --- | --- |
| TypeScript | JSX/TSX 포함 |
| JavaScript | JSX 포함 |
| Python | 동기·비동기 코드 포함 |
| Java | JVM 서버·기업 애플리케이션 중심 |
| Kotlin | JVM·Android·서버 코드 포함 |
| C# | .NET 코드 포함 |
| C | 네이티브·시스템 코드 중심 |
| C++ | 네이티브·시스템·게임 코드 중심 |
| Go | 서버·도구 코드 중심 |
| Rust | 서버·시스템·도구 코드 중심 |
| Swift | Apple 플랫폼·서버 Swift 포함 |
| PHP | 웹 애플리케이션 중심 |
| Ruby | 웹 애플리케이션 중심 |
| Dart | Flutter·서버 Dart 포함 |

D, Lua, R, MATLAB, Perl, Haskell 등 현재 grammar에 존재하는 다른 언어는 이
계약의 정밀 분석 지원 범위에 포함하지 않는다. 필요하면 이후 별도 계약 버전에서
추가한다.

## 2. 언어별 프레임워크·주요 라이브러리

여기서 프레임워크·주요 라이브러리는 API, Handler, Controller, Middleware, DI,
이벤트, 비동기 경계를 코드에서 식별하기 위한 분석 대상이다.

| 언어 | 공식 분석 대상 |
| --- | --- |
| TypeScript | React, Next.js, Angular, Vue, Nuxt, SvelteKit, Express, Fastify, NestJS, Koa |
| JavaScript | React, Next.js, Angular, Vue, Nuxt, SvelteKit, Express, Fastify, NestJS, Koa |
| Python | Django, Flask, FastAPI, Starlette, Sanic |
| Java | Spring, Spring Boot, Spring MVC, Spring WebFlux, Jakarta EE, Quarkus, Micronaut, Play |
| Kotlin | Spring Boot, Ktor, Micronaut, Android, Jetpack Compose |
| C# | ASP.NET Core, ASP.NET MVC, ASP.NET Web API, Minimal API, Blazor, .NET MAUI |
| C | GTK/GLib, Qt, libuv, libevent, gRPC |
| C++ | Qt, MFC, Boost.Asio, POCO, Unreal Engine, Drogon, Crow, gRPC |
| Go | net/http, Gin, Echo, Fiber, Chi, Beego, gRPC |
| Rust | Axum, Actix Web, Rocket, Warp, Poem, Tokio, Tonic |
| Swift | SwiftUI, UIKit, Vapor, Hummingbird, SwiftNIO |
| PHP | Laravel, Symfony, CodeIgniter, Laminas, Slim, CakePHP, API Platform |
| Ruby | Rails, Sinatra, Hanami, Rack, Grape, Roda |
| Dart | Flutter, Shelf, Serverpod, Dart Frog |

## 3. 언어별 ORM·DB 접근 도구

ORM은 코드의 객체·모델·엔티티를 DB의 테이블·행·컬럼에 연결하는 도구다. ORM이
아닌 SQL mapper, query builder, micro-ORM, typed SQL 도구도 코드에서 DB 접근을
해석해야 하므로 같은 분석 범위에 포함한다.

| 언어 | 공식 분석 대상 |
| --- | --- |
| TypeScript | Prisma, TypeORM, Sequelize, Drizzle, MikroORM, Mongoose |
| JavaScript | Prisma, Sequelize, Drizzle, Mongoose, Knex |
| Python | SQLAlchemy, Django ORM, SQLModel, Tortoise ORM, Peewee |
| Java | Hibernate, JPA, Spring Data JPA, EclipseLink, MyBatis, jOOQ |
| Kotlin | Exposed, Room, SQLDelight, Hibernate, JPA |
| C# | Entity Framework Core, Dapper, NHibernate, linq2db |
| C | ODBC, SQLite API, QtSql, SOCI |
| C++ | SQLite, SOCI, sqlpp11, ODBC |
| Go | GORM, Ent, Bun, sqlx, sqlc, SQLBoiler |
| Rust | Diesel, SeaORM, SQLx, rbatis |
| Swift | SwiftData, Core Data, Fluent, GRDB, SQLite.swift |
| PHP | Eloquent, Doctrine ORM, Propel, Cycle ORM |
| Ruby | ActiveRecord, Sequel, ROM-rb, Mongoid |
| Dart | Drift, Floor, Realm, Isar, Hive |

C와 C++는 일반적인 ORM 생태계가 약하므로 ORM 이름 매칭보다 ODBC, SQLite,
native driver, query builder, SQL 호출을 우선 분석한다.

## 4. 공통 API·메시지·외부 시스템 기술

언어에 종속되지 않고 기능 흐름과 외부 경계를 만드는 기술은 공통 어댑터로
분석한다.

| 영역 | 공식 분석 대상 |
| --- | --- |
| API 계약 | OpenAPI, Swagger, GraphQL |
| RPC·직렬화 | gRPC, Protobuf |
| 실시간 통신 | WebSocket |
| 메시지·이벤트 | Kafka, RabbitMQ, NATS |
| 캐시·외부 저장소 | Redis |
| 외부 데이터 저장소 | MongoDB |

GraphQL과 Protobuf는 API·스키마의 진입점으로 분석하고, Kafka·RabbitMQ·NATS는
producer·consumer·topic 관계로 분석한다.

## 5. 공식 DB 엔진 범위

`database-memory`가 실제 스키마의 source of truth로 지원하는 관계형 DB는 다음과
같이 고정한다.

```text
PostgreSQL
MySQL
MariaDB
SQLite
SQL Server
Oracle
```

MongoDB와 Redis는 이 계약에서 관계형 테이블·컬럼 분석 대상으로 취급하지 않는다.
코드에서 사용하는 client·collection·key 정보는 코드 측 외부 데이터 저장소
참조로 보존할 수 있지만, 관계형 `Table`·`Column` 관계로 변환하지 않는다.

## 6. 실행환경과 제외 범위

다음은 코드 프레임워크나 ORM이 아니므로 핵심 framework pack으로 만들지 않는다.

```text
Tomcat
Jetty
Undertow
Kestrel
Node.js runtime
Tokio runtime
SwiftNIO runtime
```

이들은 필요할 때 실행환경·배포 메타데이터로만 읽는다. 예를 들어 Spring은
Controller·Service·Repository·DI를 분석하지만, Tomcat 자체를 기능 흐름 노드로
만들지는 않는다.

다음 파일은 기본 기능 흐름 그래프의 노드로 만들지 않는다.

```text
HTML, CSS, SCSS
JSON, YAML, TOML, INI, XML
Markdown, RST, Typst, Mermaid
CSV, dotenv, Properties
Gitignore, Gitattributes, Diff
CMake, Makefile, Meson, Dockerfile
```

단, 설정·의존성·빌드 파일은 분석에 필요한 경우 메타데이터로 읽을 수 있다.

## 7. 지원 판정 규칙

1. 위 언어 목록에 없는 언어는 정밀 분석 지원으로 선언하지 않는다.
2. 위 프레임워크·ORM 목록에 없는 라이브러리는 일반 AST·심볼·호출 분석까지만 수행한다.
3. 공식 목록에 없는 프레임워크·ORM을 이름만 보고 자동으로 같은 의미로 처리하지 않는다.
4. 코드 엔진은 코드에 나타난 SQL·ORM·DB client 사용 정보를 보존한다.
5. 실제 테이블·컬럼·PK·FK·인덱스 존재 여부는 `database-memory`가 검증한다.
6. Tomcat 같은 실행환경은 코드 흐름의 Framework 노드로 만들지 않는다.
7. 이 목록은 UI의 확정·추정·모름 표시가 아니라 엔진이 책임지는 분석 범위 계약이다.

## 8. 완료 기준

각 공식 지원 스택마다 최소한 다음 fixture를 가져야 한다.

```text
파일·모듈·함수 추출
import·정의·참조 연결
직접 호출 연결
대표 API·Handler 연결
대표 ORM·SQL 호출 추출
테이블·컬럼 코드 참조 추출
DB 연결 시 실제 DB 객체와 통합
```

이 계약 밖의 언어와 도구는 grammar가 존재하거나 외부 분석기가 있더라도 Visual
Map 공식 지원으로 간주하지 않는다.
