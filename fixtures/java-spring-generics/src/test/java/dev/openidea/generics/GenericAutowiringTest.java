package dev.openidea.generics;

import java.util.List;
import java.util.Map;
import java.util.Optional;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.beans.factory.UnsatisfiedDependencyException;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.context.annotation.AnnotationConfigApplicationContext;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Primary;
import org.springframework.stereotype.Component;
import static org.junit.jupiter.api.Assertions.*;

class GenericAutowiringTest {
    interface Store<T> {}
    @Component("textStore") static class TextStore implements Store<String> {}
    @Component("numberStore") @Primary static class NumberStore implements Store<Integer> {}
    @Component("otherTextStore") static class OtherTextStore implements Store<String> {}

    @Component static class Client {
        final Store<String> store;
        final List<Store<String>> stores;
        final Map<String, Store<String>> map;
        final Store<String>[] array;
        final Optional<Store<String>> optional;
        final ObjectProvider<Store<String>> provider;
        Client(Store<String> numberStore, List<Store<String>> stores, Map<String, Store<String>> map,
               Store<String>[] array, Optional<Store<String>> optional, ObjectProvider<Store<String>> provider) {
            this.store = numberStore;
            this.stores = stores;
            this.map = map;
            this.array = array;
            this.optional = optional;
            this.provider = provider;
        }
    }

    @Test void genericQualifierPrecedesPrimaryAndParameterName() {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(TextStore.class, NumberStore.class, Client.class)) {
            Client client = context.getBean(Client.class);
            assertSame(context.getBean(TextStore.class), client.store);
            assertEquals(1, client.stores.size());
            assertSame(client.store, client.stores.get(0));
            assertEquals(1, client.map.size());
            assertSame(client.store, client.map.get("textStore"));
            assertEquals(1, client.array.length);
            assertSame(client.store, client.array[0]);
            assertSame(client.store, client.optional.get());
            assertSame(client.store, client.provider.getObject());
        }
    }

    abstract static class Base<T> implements Store<List<T>> {}
    abstract static class Middle<U> extends Base<U> {}
    @Component static class TextListStore extends Middle<String> {}
    @Component @Primary static class NumberListStore extends Middle<Integer> {}
    @Component static class ListClient {
        final Store<List<String>> store;
        ListClient(Store<List<String>> numberListStore) { this.store = numberListStore; }
    }

    @Test void substitutesGenericArgumentsAcrossMultipleSuperclasses() {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(TextListStore.class, NumberListStore.class, ListClient.class)) {
            assertSame(context.getBean(TextListStore.class), context.getBean(ListClient.class).store);
        }
    }

    @Configuration static class FactoryConfig {
        @Bean Store<String> textStore() { return new TextStore(); }
        @Bean @Primary Store<Integer> numberStore() { return new NumberStore(); }
    }

    @Test void factoryMethodReturnSignatureSuppliesGenericQualifier() {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(FactoryConfig.class, Client.class)) {
            assertSame(context.getBean("textStore"), context.getBean(Client.class).store);
        }
    }

    @Component static class QualifiedClient {
        final Store<String> store;
        QualifiedClient(@Qualifier("otherTextStore") Store<String> store) { this.store = store; }
    }

    @Test void explicitQualifierNarrowsMatchingGenericCandidates() {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(TextStore.class, OtherTextStore.class, NumberStore.class, QualifiedClient.class)) {
            assertSame(context.getBean(OtherTextStore.class), context.getBean(QualifiedClient.class).store);
        }
    }

    @Component static class MissingClient { MissingClient(Store<Long> store) {} }
    @Component static class SingleClient {
        final Store<String> store;
        SingleClient(Store<String> store) { this.store = store; }
    }
    @Component static class WrongQualifierClient { WrongQualifierClient(@Qualifier("numberStore") Store<String> store) {} }

    @Test void incompatibleConcreteArgumentCannotSatisfyInjection() {
        assertThrows(UnsatisfiedDependencyException.class, () -> startFailingContext(TextStore.class, NumberStore.class, MissingClient.class));
    }

    @Test void twoMatchingGenericBeansRemainAmbiguous() {
        assertThrows(UnsatisfiedDependencyException.class, () -> startFailingContext(TextStore.class, OtherTextStore.class, SingleClient.class));
    }

    @Test void qualifierCannotSelectIncompatibleGenericBean() {
        assertThrows(UnsatisfiedDependencyException.class, () -> startFailingContext(TextStore.class, NumberStore.class, WrongQualifierClient.class));
    }

    @Configuration static class RawConfig {
        @Bean Store<String> textStore() { return new TextStore(); }
        @Bean @Primary @SuppressWarnings("rawtypes") Store rawStore() { return new NumberStore(); }
    }
    @Test void concreteSignaturePrecedesRawPrimaryFallback() {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(RawConfig.class, SingleClient.class)) {
            // The source plugin deliberately retains both candidates for unresolved raw signatures.
            // The real container has enough type information to perform its strict/fallback passes.
            assertSame(context.getBean("textStore"), context.getBean(SingleClient.class).store);
        }
    }

    private void startFailingContext(Class<?>... types) {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext()) {
            context.register(types);
            context.refresh();
        }
    }
}
