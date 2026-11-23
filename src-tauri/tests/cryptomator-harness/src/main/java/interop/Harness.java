package interop;

import static java.nio.charset.StandardCharsets.UTF_8;

import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.PrintStream;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.DirectoryNotEmptyException;
import java.nio.file.DirectoryStream;
import java.nio.file.FileVisitResult;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.nio.file.SimpleFileVisitor;
import java.nio.file.StandardCopyOption;
import java.nio.file.attribute.BasicFileAttributes;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HexFormat;
import java.util.List;
import org.cryptomator.cryptofs.CryptoFileSystem;
import org.cryptomator.cryptofs.CryptoFileSystemProperties;
import org.cryptomator.cryptofs.CryptoFileSystemProperties.FileSystemFlags;
import org.cryptomator.cryptofs.CryptoFileSystemProvider;
import org.cryptomator.cryptolib.api.AuthenticationFailedException;
import org.cryptomator.cryptolib.api.CryptorProvider;
import org.cryptomator.cryptolib.api.InvalidPassphraseException;
import org.cryptomator.cryptolib.api.Masterkey;
import org.cryptomator.cryptolib.api.MasterkeyLoader;
import org.cryptomator.cryptolib.api.MasterkeyLoadingFailedException;
import org.cryptomator.cryptolib.common.MasterkeyFileAccess;

/**
 * Makes, reads and changes Cryptomator vaults with Cryptomator's own libraries, so the tests in
 * {@code tests/vault.rs} and {@code tests/vault_servers.rs} can hold Poros against them.
 *
 * <p>Only {@code dump} writes to standard output. Exit codes: 0 done, 1 failed, 2 wrong usage,
 * 3 wrong password, 4 a file or name failed authentication.
 */
public final class Harness {

    private static final String MASTERKEY_FILE = "masterkey.cryptomator";

    /** The key id the Cryptomator apps write into vault.cryptomator. */
    private static final URI KEY_ID = URI.create("masterkeyfile:" + MASTERKEY_FILE);

    /** The Cryptomator apps add no pepper to the password. */
    private static final byte[] NO_PEPPER = new byte[0];

    private static final int EXIT_FAILED = 1;
    private static final int EXIT_WRONG_USAGE = 2;
    private static final int EXIT_WRONG_PASSWORD = 3;
    private static final int EXIT_NOT_AUTHENTIC = 4;

    private static final String USAGE =
            """
            usage: java -jar harness.jar <command> <vault folder> <password> ...
              create   <SIV_GCM|SIV_CTRMAC>   make an empty vault in a new or empty folder
              populate                        fill a vault with the files of the test fixtures
              dump                            print every folder, link and file with its SHA-256
              put      <path> <local file>    write a file, making its folders
              mkdir    <path>                 make a folder and its parents
              move     <from> <to>            rename or move a file or folder
              delete   <path>                 delete a file, or a folder with what it holds
              symlink  <path> <target>        make a symbolic link
            Paths are absolute paths inside the vault, such as /Documents/a.txt.
            """;

    /** UTF-8 whatever the locale, which is ASCII on many build machines. */
    private static final PrintStream OUTPUT =
            new PrintStream(new FileOutputStream(FileDescriptor.out), false, UTF_8);

    private static final PrintStream ERRORS =
            new PrintStream(new FileOutputStream(FileDescriptor.err), true, UTF_8);

    private Harness() {}

    public static void main(String[] arguments) {
        int exitCode = 0;
        try {
            requireUtf8(arguments);
            run(arguments);
        } catch (WrongUsageException wrongUsage) {
            ERRORS.println(wrongUsage.getMessage());
            ERRORS.print(USAGE);
            exitCode = EXIT_WRONG_USAGE;
        } catch (IOException | RuntimeException | NoSuchAlgorithmException failure) {
            failure.printStackTrace(ERRORS);
            exitCode = exitCodeOf(failure);
        }
        OUTPUT.flush();
        System.exit(exitCode);
    }

    private static int exitCodeOf(Throwable failure) {
        for (Throwable cause = failure; cause != null; cause = cause.getCause()) {
            if (cause instanceof InvalidPassphraseException) {
                return EXIT_WRONG_PASSWORD;
            }
            if (cause instanceof AuthenticationFailedException) {
                return EXIT_NOT_AUTHENTIC;
            }
        }
        return EXIT_FAILED;
    }

    /**
     * The JVM decodes its arguments with the locale's character set, and under an ASCII locale
     * turns every other character of a password or path into a replacement character without a
     * word. Such arguments are refused instead of used.
     */
    private static void requireUtf8(String[] arguments) throws IOException {
        String decodedAs = System.getProperty("sun.jnu.encoding", "");
        boolean utf8 = decodedAs.equalsIgnoreCase("UTF-8") || decodedAs.equalsIgnoreCase("UTF8");
        boolean ascii =
                Arrays.stream(arguments)
                        .allMatch(StandardCharsets.US_ASCII.newEncoder()::canEncode);
        if (!utf8 && !ascii) {
            throw new IOException(
                    "The arguments were decoded as " + decodedAs + ": run with LC_ALL=C.UTF-8");
        }
    }

    private static void run(String[] arguments) throws IOException, NoSuchAlgorithmException {
        if (arguments.length < 3) {
            throw new WrongUsageException("A command, a vault folder and a password are needed");
        }
        String command = arguments[0];
        Path vaultFolder = Path.of(arguments[1]).toAbsolutePath();
        String password = arguments[2];
        List<String> rest = List.of(arguments).subList(3, arguments.length);
        int expected =
                switch (command) {
                    case "populate", "dump" -> 0;
                    case "create", "mkdir", "delete" -> 1;
                    case "put", "move", "symlink" -> 2;
                    default -> throw new WrongUsageException("Unknown command: " + command);
                };
        if (rest.size() != expected) {
            throw new WrongUsageException(
                    command + " takes " + expected + " more, not " + rest.size());
        }
        if (command.equals("create")) {
            create(vaultFolder, password, cipherCombination(rest.get(0)));
            return;
        }
        boolean readOnly = command.equals("dump");
        try (CryptoFileSystem vault = open(vaultFolder, password, readOnly)) {
            switch (command) {
                case "populate" -> populate(vault);
                // Printed only once the whole vault has been read and authenticated.
                case "dump" -> OUTPUT.print(dump(vault));
                case "put" -> put(inVault(vault, rest.get(0)), Path.of(rest.get(1)));
                case "mkdir" -> Files.createDirectories(inVault(vault, rest.get(0)));
                case "move" -> Files.move(inVault(vault, rest.get(0)), inVault(vault, rest.get(1)));
                case "delete" -> delete(inVault(vault, rest.get(0)));
                case "symlink" ->
                        Files.createSymbolicLink(
                                inVault(vault, rest.get(0)), vault.getPath(rest.get(1)));
                default -> throw new IllegalStateException(command);
            }
        }
    }

    private static CryptorProvider.Scheme cipherCombination(String name) {
        try {
            return CryptorProvider.Scheme.valueOf(name);
        } catch (IllegalArgumentException unknown) {
            throw new WrongUsageException("Unknown cipher combination: " + name);
        }
    }

    /** Makes a vault as the Cryptomator apps do, without the readme files they add. */
    private static void create(Path vaultFolder, String password, CryptorProvider.Scheme scheme)
            throws IOException, NoSuchAlgorithmException {
        Files.createDirectories(vaultFolder);
        try (DirectoryStream<Path> existing = Files.newDirectoryStream(vaultFolder)) {
            if (existing.iterator().hasNext()) {
                throw new DirectoryNotEmptyException(vaultFolder.toString());
            }
        }
        SecureRandom random = SecureRandom.getInstanceStrong();
        try (Masterkey masterkey = Masterkey.generate(random)) {
            new MasterkeyFileAccess(NO_PEPPER, random)
                    .persist(masterkey, vaultFolder.resolve(MASTERKEY_FILE), password);
            MasterkeyLoader loader = keyId -> masterkey.copy();
            CryptoFileSystemProperties properties =
                    CryptoFileSystemProperties.cryptoFileSystemProperties()
                            .withCipherCombo(scheme)
                            .withKeyLoader(loader)
                            .build();
            CryptoFileSystemProvider.initialize(vaultFolder, properties, KEY_ID);
        }
    }

    /** Opens a vault with the masterkey file its configuration names, as the apps do. */
    private static CryptoFileSystem open(Path vaultFolder, String password, boolean readOnly)
            throws IOException, NoSuchAlgorithmException {
        MasterkeyFileAccess masterkeyFiles =
                new MasterkeyFileAccess(NO_PEPPER, SecureRandom.getInstanceStrong());
        MasterkeyLoader loader =
                keyId -> {
                    if (!"masterkeyfile".equals(keyId.getScheme())) {
                        throw new MasterkeyLoadingFailedException("Unsupported key: " + keyId);
                    }
                    Path masterkeyFile = vaultFolder.resolve(keyId.getSchemeSpecificPart());
                    return masterkeyFiles.load(masterkeyFile, password);
                };
        var properties =
                CryptoFileSystemProperties.cryptoFileSystemProperties().withKeyLoader(loader);
        if (readOnly) {
            properties.withFlags(FileSystemFlags.READONLY);
        }
        return CryptoFileSystemProvider.newFileSystem(vaultFolder, properties.build());
    }

    private static Path inVault(CryptoFileSystem vault, String path) {
        if (!path.startsWith("/")) {
            throw new WrongUsageException("A path in the vault starts with /: " + path);
        }
        return vault.getPath(path);
    }

    private static void put(Path target, Path localFile) throws IOException {
        if (target.getParent() != null) {
            Files.createDirectories(target.getParent());
        }
        try (InputStream contents = Files.newInputStream(localFile)) {
            Files.copy(contents, target, StandardCopyOption.REPLACE_EXISTING);
        }
    }

    private static void delete(Path path) throws IOException {
        if (path.getNameCount() == 0) {
            throw new IOException("The vault's own folder is not deleted");
        }
        // Links are not followed: a link is deleted, never what it points to.
        Files.walkFileTree(
                path,
                new SimpleFileVisitor<>() {
                    @Override
                    public FileVisitResult visitFile(Path file, BasicFileAttributes attributes)
                            throws IOException {
                        Files.delete(file);
                        return FileVisitResult.CONTINUE;
                    }

                    @Override
                    public FileVisitResult postVisitDirectory(Path folder, IOException failure)
                            throws IOException {
                        if (failure != null) {
                            throw failure;
                        }
                        Files.delete(folder);
                        return FileVisitResult.CONTINUE;
                    }
                });
    }

    /** Bytes that differ from one position and one file to the next. */
    private static byte[] pattern(int length, int seed) {
        byte[] bytes = new byte[length];
        for (int index = 0; index < length; index++) {
            bytes[index] = (byte) (index * 31 + seed);
        }
        return bytes;
    }

    private static byte[] text(String text) {
        return text.getBytes(StandardCharsets.US_ASCII);
    }

    /**
     * Writes what the fixtures in {@code tests/fixtures/vaults} hold: files around the size of a
     * chunk, names in several scripts and normalization forms, names long enough to be stored
     * shortened, links, and a folder of many files.
     */
    private static void populate(CryptoFileSystem vault) throws IOException {
        // Spelled with escapes, so the normalization form of each name is the one meant.
        String composedFolder = "Ünïcödé";
        String cyrillicFile = "файл.txt";
        String japaneseFolder = "日本語";
        String japaneseFile = "こんにちは.txt";
        String decomposedFile = "été.txt";
        String longFile = "long-file-" + "x".repeat(186) + ".txt";
        String longFolder = "long-dir-" + "y".repeat(191);

        Files.write(vault.getPath("/empty.txt"), new byte[0]);
        Files.write(vault.getPath("/hello.txt"), text("Hello, Cryptomator!\n"));
        Files.write(vault.getPath("/chunk-exact.bin"), pattern(32768, 1));
        Files.write(vault.getPath("/chunk-plus-one.bin"), pattern(32769, 2));
        Files.write(vault.getPath("/chunk-minus-one.bin"), pattern(32767, 3));
        Files.write(vault.getPath("/three-chunks.bin"), pattern(98304, 4));
        Files.write(vault.getPath("/large.bin"), pattern(250000, 5));
        Files.write(vault.getPath("/.hidden"), text("dot\n"));

        Files.createDirectories(vault.getPath("/Documents/Reports/2024"));
        Files.write(vault.getPath("/Documents/Reports/2024/q1.txt"), text("quarterly\n"));
        Files.createDirectories(vault.getPath("/empty-dir"));
        Files.createDirectories(vault.getPath("/a b"));
        Files.write(vault.getPath("/a b/c & d (1).txt"), text("spaces\n"));

        Files.createDirectories(vault.getPath("/", composedFolder));
        Files.write(vault.getPath("/", composedFolder, cyrillicFile), text("unicode\n"));
        Files.createDirectories(vault.getPath("/", japaneseFolder));
        Files.write(vault.getPath("/", japaneseFolder, japaneseFile), text("japanese\n"));
        Files.createDirectories(vault.getPath("/nfd"));
        Files.write(vault.getPath("/nfd", decomposedFile), text("nfd\n"));

        Files.write(vault.getPath("/", longFile), text("long file\n"));
        Files.createDirectories(vault.getPath("/", longFolder));
        Files.write(vault.getPath("/", longFolder, "inside.txt"), text("inside long dir\n"));

        Files.createSymbolicLink(vault.getPath("/link-to-hello"), vault.getPath("hello.txt"));
        Files.createSymbolicLink(
                vault.getPath("/Documents/link-up"), vault.getPath("../hello.txt"));

        Files.createDirectories(vault.getPath("/many"));
        for (int number = 0; number < 40; number++) {
            String name = String.format("file-%02d.txt", number);
            Files.write(vault.getPath("/many", name), text(name + "\n"));
        }
    }

    /** An entry of a dump, with a size and SHA-256 if it is a file and a target if a link. */
    private record Listed(String path, String type, long size, String sha256, String target) {

        String toJson() {
            String fields = "\"path\": " + quoted(path) + ", \"type\": " + quoted(type);
            String more =
                    switch (type) {
                        case "file" -> ", \"size\": " + size + ", \"sha256\": " + quoted(sha256);
                        case "symlink" -> ", \"target\": " + quoted(target);
                        default -> "";
                    };
            return "{" + fields + more + "}";
        }
    }

    /**
     * Everything in the vault as a JSON array, an entry to a line, sorted by path. Every file is
     * read to its end, so a vault with a damaged file is not dumped.
     */
    private static String dump(CryptoFileSystem vault)
            throws IOException, NoSuchAlgorithmException {
        List<Listed> everything = new ArrayList<>();
        list(vault.getPath("/"), everything);
        everything.sort(
                (first, second) ->
                        Arrays.compareUnsigned(
                                first.path().getBytes(UTF_8), second.path().getBytes(UTF_8)));
        if (everything.isEmpty()) {
            return "[]\n";
        }
        List<String> lines = everything.stream().map(Listed::toJson).toList();
        return "[\n  " + String.join(",\n  ", lines) + "\n]\n";
    }

    private static void list(Path folder, List<Listed> everything)
            throws IOException, NoSuchAlgorithmException {
        List<Path> children = new ArrayList<>();
        try (DirectoryStream<Path> entries = Files.newDirectoryStream(folder)) {
            entries.forEach(children::add);
        }
        for (Path child : children) {
            BasicFileAttributes attributes =
                    Files.readAttributes(
                            child, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS);
            String path = child.toString();
            if (attributes.isSymbolicLink()) {
                String target = Files.readSymbolicLink(child).toString();
                everything.add(new Listed(path, "symlink", 0, null, target));
            } else if (attributes.isDirectory()) {
                everything.add(new Listed(path, "dir", 0, null, null));
                list(child, everything);
            } else {
                everything.add(hashed(child, attributes.size()));
            }
        }
    }

    private static Listed hashed(Path file, long listedSize)
            throws IOException, NoSuchAlgorithmException {
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        long size = 0;
        byte[] buffer = new byte[1 << 16];
        try (InputStream contents = Files.newInputStream(file)) {
            for (int read = contents.read(buffer); read != -1; read = contents.read(buffer)) {
                digest.update(buffer, 0, read);
                size += read;
            }
        }
        if (size != listedSize) {
            throw new IOException(
                    file + " is listed with " + listedSize + " bytes and holds " + size);
        }
        String sha256 = HexFormat.of().formatHex(digest.digest());
        return new Listed(file.toString(), "file", size, sha256, null);
    }

    /** A JSON string. Names are written as they are, in UTF-8, apart from what JSON must escape. */
    private static String quoted(String text) {
        StringBuilder json = new StringBuilder("\"");
        text.codePoints()
                .forEach(
                        character -> {
                            if (character == '"' || character == '\\') {
                                json.append('\\').appendCodePoint(character);
                            } else if (character < 0x20 || character == 0x7F) {
                                json.append(String.format("\\u%04x", character));
                            } else {
                                json.appendCodePoint(character);
                            }
                        });
        return json.append('"').toString();
    }

    private static final class WrongUsageException extends RuntimeException {
        WrongUsageException(String message) {
            super(message);
        }
    }
}
